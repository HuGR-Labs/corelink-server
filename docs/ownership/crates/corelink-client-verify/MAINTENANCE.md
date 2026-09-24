---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-client-verify
manifest: crates/corelink-client-verify/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: candidate
evidence_set: client-verify-source-static-16d9f0303
---

# corelink-client-verify — maintenance

S-profile maintenance is SOURCE/DOCUMENTARY only. It does not authorize or
represent Cargo, test, SDK/client, ABI, network, runtime, deploy, or review
execution as evidence.

[Baseline](#m01) · [Root/config](#m02) · [Verify/error](#m03) · [Feature modes](#m04) · [Validate](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Establish the static baseline

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Assigned worktree and pinned revision. | Package, manifest, and exactly four owned artifacts match the assignment. | Read revision and paths; name all four before source review. | Stop on baseline/path mismatch; recover only by reconciling scope. | Revision and paths; documentary/source only. |

<a id="m02"></a>
## M02 — Review root or configuration change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Changed root export, feature gate, config field, or constructor is named. | Each changed gate/export/config literal has one falsifier and R02/R03/B01 impact. | Read `Cargo.toml`, `src/lib.rs`, and `src/config.rs`; record one predicate per changed item. | Stop when a consumer choice, compile, or warning observation is required; retain as UNKNOWN. | Named manifest/source ranges. |

<a id="m03"></a>
## M03 — Review verifier or error change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Changed verifier, digest import, or error symbol is named. | Config branch, digest call, comparison call, error variant, and direct B02/B03 arrow are separately recorded. | Read `verifier.rs`, `error.rs`, and `digest.rs`; update only affected atomic records. | Stop when digest behavior, body provenance, caller handling, or telemetry is required; retain as UNKNOWN. | Named local source ranges. |

<a id="m04"></a>
## M04 — Review a feature-mode change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Changed `stream` or `ffi` declaration/gated source is named. | Feature declaration, root gate, local imports, and failure/tag branch are distinct R06/R07 and B04/B05 records. | Read `Cargo.toml`, `lib.rs`, then the changed feature module. | Stop when selected features, reader I/O, pointer validity, library loading, or ABI compatibility is required; retain as UNKNOWN. | Named manifest/source ranges. |

<a id="m05"></a>
## M05 — Validate the ownership set

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Exactly four owned artifacts and the supplied profile-S checker. | S01–S07, R01–R08, B01–B06, and M01–M06 exist; checker runs once per artifact; diff has no whitespace error. | Run supplied S checker four times and `git diff --check` against `16d9f0303`; correct only owned files. | Stop on checker/diff/scope failure; recover only the reported owned file. | Checker output and diff status; documentary, not semantic approval. |

<a id="m06"></a>
## M06 — Handoff with evidence limits

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Scoped commit and M05 documentary result. | Handoff names baseline, paths, affected R/B/M IDs, check status, OKF route, and unknowns. | State only static/documentary results and direct arrows. | Stop before claiming SDK/client outcome, ABI use, compile/test, network, runtime, deploy, or independent review; state UNKNOWN. | Commit, paths, checker/diff output. |

The verified canonical OKF route remains [SDK reference](../../../knowledge/ops/sdk-reference.md); route to it without copying or revalidating policy.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)
