---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-go
manifest: tools/sdks/go/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: candidate
evidence_set: corelink-go-source-static-3feae2b
---

# corelink-go — maintenance

S-profile maintenance is SOURCE/DOCUMENTARY only. It does not authorize or
represent Cargo, tests, Go/cgo, linker or ABI execution, client use, release,
network, runtime, deployment, or review evidence.

[Baseline](#m01) · [Manifest/root](#m02) · [Handle](#m03) · [Entries](#m04) · [Validate](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Establish the static baseline

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Assigned worktree and pinned revision. | Package, manifest, and exactly four owned artifacts match the assignment. | Read revision and the four paths before source review. | Stop on baseline/path mismatch; recover only by reconciling scope. | Revision and paths; documentary/source only. |

<a id="m02"></a>
## M02 — Review manifest or root change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | A crate type, dependency, root module, re-export, Go C declaration, or cgo call-site change is named. | Each changed declaration/call has one falsifier and R01/R02/R08/B01/B02/B06 impact. | Read `Cargo.toml`, `src/lib.rs`, and affected `corelink-go/corelink.go` range; record each wrapper-to-C call separately. | Stop when feature resolution, artifact/linker state, symbol emission, pointer safety, or actual cgo use is required; retain as UNKNOWN. | Named manifest/Rust/Go source ranges. |

<a id="m03"></a>
## M03 — Review handle construction or lifecycle change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | A constructor, verifier-selection, field, free, or accessor change is named. | Validation, default/disabled branch, stored field, free call, accessor, and matching Go wrapper call are distinct R03–R05/B03–B04/B06 records. | Read affected `go_bridge.rs` and `corelink-go/corelink.go` ranges plus dependency imports. | Stop when pointer validity, logs/counters, thread behavior, cgo generation, or Go/cgo ownership is required; retain as UNKNOWN. | Named Rust/Go source ranges. |

<a id="m04"></a>
## M04 — Review digest or verify entry change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | A digest/verify guard, call, result code, or pointer parameter changes. | Each local guard, `Digest` call/copy, delegated verifier argument, and Go wrapper call is separately recorded in R06/R07/R08 and B05/B06. | Read the changed Rust entry and matching Go call site; preserve the declared unknown boundary. | Stop when buffer safety, algorithm behavior, return handling, ABI compatibility, upload/download, or executed client behavior is required; retain as UNKNOWN. | Named Rust/Go source ranges. |

<a id="m05"></a>
## M05 — Validate the ownership set

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Exactly four owned artifacts and the controlled profile-S checker. | S01–S07, R01–R08, B01–B06, and M01–M06 exist; checker runs once per artifact; diff has no whitespace error. | Run the four controlled commands below and `git diff --check 3feae2baed63061354533ffdfe2d94acfd24aa9a`; correct only owned files. | Stop on checker/diff/scope failure; recover only the reported owned file. | Checker output and diff status; documentary, not semantic approval. |

```sh
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-go/SKILL.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-go/REFERENCE.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-go/BLAST_RADIUS.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-go/MAINTENANCE.md
```

Each JSON `IMPLEMENTED_CHECKS_PASS` result is structural-only. It does not
certify semantic completeness, runtime claims, cold review, or profile
eligibility.

<a id="m06"></a>
## M06 — Handoff with evidence limits

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Scoped commit and M05 documentary result. | Handoff names baseline, paths, affected R/B/M IDs, check status, OKF route, and unknowns. | State only static/documentary results and direct arrows. | Stop before claiming Cargo/test, Go/cgo, client, linker/ABI, release, network, runtime, deployment, or independent review; state UNKNOWN. | Commit, paths, checker/diff output. |

The verified canonical OKF route remains [SDK reference](../../../knowledge/ops/sdk-reference.md); route to it without copying or revalidating policy.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)
