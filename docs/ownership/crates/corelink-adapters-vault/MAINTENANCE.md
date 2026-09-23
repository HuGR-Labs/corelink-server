---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-adapters-vault
manifest: crates/corelink-adapters-vault/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-adapters-vault-structural-normalization-20260921
---

# corelink-adapters-vault — maintenance

Static-analysis maintenance guide for the canonical facade only. It does not
authorize Cargo execution, feature resolution, target builds, provider calls,
Vault/mTLS access, certificate/secret/key handling, migration operation,
deployment, or self/cold review.

[Baseline](#m01) · [Scope](#m02) · [Facade path](#m03) · [Feature](#m04) ·
[Impact](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Baseline and mode

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | Assigned worktree, expected baseline, manifest, and two local source files are identified | Assessment is limited to the recorded facade at the stated revision | Stop on divergent baseline; recover by obtaining the reconciled baseline without reset | Git SHA, manifest, source paths; SOURCE |

<a id="m02"></a>
## M02 — Classify the requested change

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | Requested symbol/path is known | It classifies as local module wiring, direct re-export, or dependency feature declaration | Stop if it changes a BYOK provider or asks for runtime/migration proof; recover by routing to that owner | R03–R04; SOURCE |

<a id="m03"></a>
## M03 — Change the public facade path

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | The target is `src/lib.rs` or `src/vault.rs` | The local public module and exact provider re-export remain explicit; source-level impact is recorded | Stop if a change copies or changes provider logic; recover with a `corelink-byok` owner decision | R04–R05, B02; SOURCE |

<a id="m04"></a>
## M04 — Change the feature declaration

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | The target is the `corelink-byok` dependency feature declaration | The adapter declaration and BYOK feature/namespace relation are recorded separately | Stop before inferring feature resolution, provider selection, or a compiled target; recover by requesting resolved/build evidence | R06, B01/B03; SOURCE |

<a id="m05"></a>
## M05 — Assess static impact and migration boundary

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | Changed path/feature and bounded search scope are recorded | Workspace membership, aliases, direct manifest consumers, and migration metadata are classified separately | Stop if complete consumer closure, migration completion, or an operational result is required; recover by recording the gap and requesting direct evidence | B04–B06; SOURCE |

<a id="m06"></a>
## M06 — Static handoff and recovery boundary

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | Documentation/change assessment is complete | Handoff states baseline, local source paths, facade/feature relations, provider boundary, static consumers, and unknowns | Stop before claiming a provider ran, Vault/mTLS or key behavior occurred, a FIPS property held, a migration completed, or review passed; recover by retaining SOURCE evidence and escalating the missing proof | R08, B06; SOURCE |

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
