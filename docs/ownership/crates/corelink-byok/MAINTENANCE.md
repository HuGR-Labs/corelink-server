---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-byok
manifest: crates/corelink-byok/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: H
state: author_validated
evidence_set: corelink-byok-structural-normalization-20260921
---

# corelink-byok — maintenance

H-profile source/static maintenance guide. It authorizes source and ownership
document assessment only. It does not authorize Cargo compilation, tests,
provider selection, KMS/key use, HTTP activity, cryptographic execution,
runtime operation, deployment, or publication.

[Baseline](#m01) · [Classification](#m02) · [Core and revocation](#m03) ·
[Features and gates](#m04) · [Document validation](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Baseline and evidence mode

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_H | Assigned worktree, expected revision, manifest, and target source path | Every conclusion is classed as source text, documentary check, or unknown | Stop on a divergent baseline or package; recover by obtaining reconciled scope without reset | Git revision; `Cargo.toml`; named source paths; SOURCE |

<a id="m02"></a>
## M02 — Classify the affected route

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_H | A changed `corelink_byok` path or module is known | It is classified as root core, revocation, provider namespace, or feature/gate declaration | Stop if a source path is used to claim provider selection or execution; recover by naming the source relation and retaining the operational result as unknown | `src/lib.rs`; R03–R04; SOURCE |

<a id="m03"></a>
## M03 — Assess core or revocation source

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_H | A changed core or revocation source path | The local module, public route, exposed port/type, and static consumers are recorded separately | Stop if key material, encryption/decryption, KMS access, D1/store action, scheduling, or alert delivery must be proved; recover by routing the operational request to its composition/provider owner | `src/byok_core.rs`, `src/byok_revocation.rs`, child trees, B01/B05; SOURCE |

<a id="m04"></a>
## M04 — Assess provider features, targets, and exclusion gates

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_H | A changed provider, feature, target table, or `compile_error!` predicate | Public feature mapping, internal module gate, namespace gate, target declaration, and all affected pair predicates are recorded | Stop if a selected feature/target, provider, HTTP client, credential, or build result is required; recover by retaining only the declarations and requesting resolved/executed evidence | `Cargo.toml`; `src/lib.rs`; R06; B03–B04; SOURCE |

<a id="m05"></a>
## M05 — Validate ownership documents

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_H | Exactly the four assigned ownership artifacts and the integration-supplied H-profile checker | Skill has S01–S07, reference R01–R08, blast radius B01–B06, maintenance M01–M06; each passes its matching documentary check and the diff has no whitespace error | Stop on checker, diff, or scope failure; recover only in the four owned paths, otherwise hand off the exact finding | Four H-profile structural checks; `git diff --check`; documentary evidence only |

<a id="m06"></a>
## M06 — Static handoff and unknowns

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_H | Scoped source/document work is complete | Handoff names baseline, changed paths, affected API/INV/relations, checker results, consumer references, canonical OKF route, and unknowns | Stop before representing author checks as Cargo/test/runtime/cold-review proof; recover by restating the claim at its evidence boundary | Repository paths, revision, validation output; SOURCE/documentary |

The verified OKF BYOK reference remains canonical and is routed, not
revalidated: [BYOK envelope encryption at rest](../../../knowledge/storage/byok-envelope-encryption.md).
Unknowns include resolved features, selected target/provider, key and
credential handling, HTTP/KMS/crypto activity, durable effects, tests,
runtime, deployment, and independent review.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)
