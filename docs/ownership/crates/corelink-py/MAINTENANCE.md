---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-py
manifest: tools/sdks/python/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: candidate
evidence_set: corelink-py-source-static-20260920
---

# corelink-py — maintenance

S-profile maintenance is SOURCE/DOCUMENTARY only. It does not authorize or represent Cargo, test, generated binding, Python client/import, network, package release, runtime, deployment, or review execution as evidence.

[Baseline](#m01) · [Packaging/manifest/bridge](#m02) · [Verify/local paths](#m03) · [Unknowns](#m04) · [Validate](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Establish the static baseline

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Assigned worktree and pinned revision. | Package, manifest, and exactly four owned artifacts match the assignment. | Read revision and name all four paths before source review. | Stop on baseline/path mismatch; recover only by reconciling scope. | Revision and paths; source/documentary only. |

<a id="m02"></a>
## M02 — Review packaging, manifest, or bridge change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Changed `pyproject.toml`, `build.rs`, crate type, feature, dependency, annotation, type, or registration is named. | Each packaging declaration, linker gate, declaration, or registration has one falsifier and separate R01/R02/R03/R06 plus B01/B02/B03 impact; dependency changes route through B02, while maturin and conditional Apple cdylib linker changes route through B01. | Read `pyproject.toml`, `Cargo.toml`, `build.rs`, and `src/lib.rs`; update only affected atomic records. | Stop when backend/feature resolution, build-script execution, linker outcome, generated binding, build, or Python import is needed; retain UNKNOWN. | Named local config/source text. |

<a id="m03"></a>
## M03 — Review verify or local-operation change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Changed constructor, verifier call, digest call, stub, error mapping, or placeholder is named. | Default branch, disabled branch, named call, and local result are separately recorded in R04/R05 and B02/B04. | Read the affected `src/lib.rs` branch and dependency declaration; preserve source boundary. | Stop when logger delivery, verifier result, transport, persistence, or client behavior is needed; retain UNKNOWN. | Named source text. |

<a id="m04"></a>
## M04 — Route an operational unknown

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| ROUTE_ONLY | A question exceeds local source evidence. | The question is named UNKNOWN and is routed without copying/revalidating policy. | Use [SDK reference](../../../knowledge/ops/sdk-reference.md) as canonical OKF context; identify the owning client/release/runtime evidence needed. | Stop before treating OKF, README, examples, docs, or package-index text as proof for this binding. Recover by retaining the unknown and escalating to the appropriate owner. | Explicit unknown and route, not local SOURCE proof. |

<a id="m05"></a>
## M05 — Validate the ownership set

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Exactly four owned artifacts and supplied S-profile checker. | S01–S07, R01–R08, B01–B06, M01–M06 exist; checker runs once per artifact; diff has no whitespace/scope error. | Run checker four times and `git diff --check` against `3feae2baed63061354533ffdfe2d94acfd24aa9a`; correct only owned files. | Stop on checker/diff/scope failure; recover only the reported owned file. | Checker output and diff status; documentary, not semantic approval. |

<a id="m06"></a>
## M06 — Handoff with evidence limits

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Scoped commit and M05 documentary result. | Handoff names baseline, paths, affected R/B/M IDs, check status, OKF route, and unknowns. | State only source/documentary results and direct arrows, including declared Python packaging metadata and conditional Apple linker arguments where relevant. | Stop before claiming client/release/import/build/link/test/network/runtime/deploy or independent-review evidence; state UNKNOWN. | Commit, paths, checker/diff output. |

The verified canonical OKF route remains [SDK reference](../../../knowledge/ops/sdk-reference.md); route to it without copying or revalidating policy.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)
