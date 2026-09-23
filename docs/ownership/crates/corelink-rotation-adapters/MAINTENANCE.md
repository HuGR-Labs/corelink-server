---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-rotation-adapters
manifest: crates/corelink-rotation-adapters/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: w009-rotation-adapters-source-static-20260920
---

# corelink-rotation-adapters — maintenance

These procedures are SOURCE/DOCUMENTARY only. They do not authorize or report
Cargo, tests, network, provider, key, rotation, deployment, or runtime work.
The verified OKF manifest query has no match; do not manufacture a route.

[Baseline](#m01) · [Root/type](#m02) · [Predicate/trait](#m03) · [Adapter/error](#m04) · [Validate](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Establish the source baseline

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_S | Assigned worktree and pinned revision | `HEAD` begins at `6be030999`; only four assigned ownership artifacts may change | Stop on a divergent baseline/scope; obtain a reconciled assignment, never reset | Git revision/status; documentary |

<a id="m02"></a>
## M02 — Assess a root or type change

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_S | A root export, schema literal, enum, field, or mapping method is named | The exact declaration, falsifier, and B02/B03 relation are recorded | Stop if a caller, compatibility result, or persisted value is required; retain it as UNKNOWN | `src/{lib,types}.rs`; SOURCE |

<a id="m03"></a>
## M03 — Assess a predicate or trait change

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_S | A predicate, trait signature, or transition pair is named | The matching expression/signature/pair list and B04 relation are separately recorded | Stop if a key or rotation outcome is requested; retain it as UNKNOWN | `src/adapter.rs`; SOURCE |

<a id="m04"></a>
## M04 — Assess an adapter, error, or test declaration

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_S | A named local adapter struct, implementation block, error variant, or test declaration changes | R06 and B05 identify the exact module/contract; test text remains labelled declared source | Stop if construction, invocation, provider behavior, or test result is required; retain it as UNKNOWN | Named local module, `error.rs`, manifest/test text; SOURCE |

<a id="m05"></a>
## M05 — Validate the ownership set

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence |
|---|---|---|---|---|
| DOCUMENTARY | Exactly four assigned artifacts and the supplied external profile-S checker are available | S01–S07, R01–R08, B01–B06, and M01–M06 validate once each; `git diff --check 6be030999` exits zero | Stop on any checker/diff/scope failure; correct only one of the four owned artifacts | Checker output and diff status; not semantic approval |

Use the checked-in structural checker once per kind:

```sh
python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-rotation-adapters/SKILL.md
python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-rotation-adapters/REFERENCE.md
python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-rotation-adapters/BLAST_RADIUS.md
python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-rotation-adapters/MAINTENANCE.md
git diff --check 6be030999
```

<a id="m06"></a>
## M06 — Handoff with unknowns

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_S | Scoped commit and M05 documentary results | Handoff names baseline, four paths, affected R/B/M IDs, no-match OKF route, and the five R08 unknowns | Stop before representing source/documentary checks as operation or cold review; state UNKNOWN | Commit, paths, checker and diff output |

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)
