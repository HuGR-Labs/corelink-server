---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-crypto
manifest: crates/corelink-crypto/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-crypto-structural-normalization-20260921
---

# corelink-crypto — maintenance

Static-analysis maintenance guide only. It does not prescribe Cargo execution,
cryptographic operations, SDK generation, FFI use, network access, deployment,
or self/cold review.

[Baseline](#m01) · [Scope](#m02) · [Provider path](#m03) · [Local facade](#m04) ·
[Consumer review](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Baseline and mode

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | Assigned worktree, expected baseline, and manifest are identified | The assessed files are the six facade sources at the recorded commit | Stop on divergent baseline; recover by obtaining the reconciled baseline without reset | Git SHA, branch, manifest/source paths; SOURCE |

<a id="m02"></a>
## M02 — Classify the requested change

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | Requested symbol is known | It maps to one provider re-export or one local facade in R03 | Stop if it alters provider logic or a foreign artifact; recover by routing to that owner | `src/lib.rs`, R03; SOURCE |

<a id="m03"></a>
## M03 — Change a provider re-export

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | The target is `blake3`, `ed25519::attestation`, or `client_verify` | The exact provider-to-facade relation in B02 remains explicit and affected static consumers are listed | Stop if the change requires copied provider source, SDK header/ABI work, or semantic proof; recover with a provider/SDK owner decision | Matching module, provider manifest/root, B02/B04–B05; SOURCE |

<a id="m04"></a>
## M04 — Change a local primitive facade

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | The target is `ct_eq` or `hmac` | Re-exported names remain enumerated and `HmacSha256` is checked against INV-004 when relevant | Stop if the requested guarantee concerns timing, key handling, or protocol verification; recover by naming the consuming contract and leaving the operational claim unknown | `ct_eq.rs`, `hmac.rs`, R05, B03/B05; SOURCE |

<a id="m05"></a>
## M05 — Assess static consumer impact

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | Changed namespace/alias and a bounded search scope are recorded | Direct dependencies, canonical-path references, and adjacent provider users are classified separately | Stop if reverse-graph closure or runtime behavior is required; recover by recording the gap and requesting resolved/runtime evidence | B04–B06 and cited manifests/imports; SOURCE |

<a id="m06"></a>
## M06 — Static handoff and recovery boundary

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | Documentation/change assessment is complete | Handoff states baseline, paths, changed API/INV/relations, direct consumers, adjacent paths, and unknowns | Stop before saying a provider executed, a SDK/FFI artifact was produced, or review passed; recover by retaining source evidence and escalating the missing proof | REFERENCE R08; BLAST B06; SOURCE |

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
