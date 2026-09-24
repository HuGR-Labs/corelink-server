---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-client-verify-fuzz
manifest: crates/corelink-client-verify/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: client-verify-fuzz-source-static-20260921
---

# corelink-client-verify-fuzz — maintenance

This manual covers bounded source review and document checks. The current ownership pass performs no Cargo, Rust, test, build, fuzz, network, GitHub, provider, production, database, or storage operation.

[Preparation](#m01) · [Choose](#m02) · [Procedures](#m03) ·
[Validation](#m04) · [Recovery](#m05) · [Escalation](#m06).

<a id="m01"></a>
## M01 — Prepare a bounded review

**Mode:** READ_ONLY. **Prerequisites:** isolated assigned worktree; explicit integration baseline and source pin; the four paths listed in the wave plan. **Predicate:** `HEAD` matches baseline before edits and package inputs match the pin or drift is recorded. **Stop:** source/baseline differs, scope expands, or an owner-dependent action is requested. **Recovery:** preserve work and report exact paths and SHAs; do not reset or switch branches. **Evidence:** baseline, pin, scoped diff, and `git status --short`.

<a id="m02"></a>
## M02 — Select the procedure

| Situation | Procedure | Mode | Effect / stop |
|---|---|---|---|
| Author or revise these four ownership artifacts | [PROC-001](#proc-001) | READ_ONLY / DOCUMENTARY | Local Markdown checks only; stop on extra paths |
| Change fuzz manifest, Rust target, CI, or parent library | No procedure in this pass | BLOCKED | Requires separately authorized scope; do not execute here |
| Ask whether a target ran, reached code, found bugs, or protects runtime | No procedure in this pass | BLOCKED | Requires independent run evidence and actual owner; this manual cannot answer |

<a id="m03"></a>
## M03 — Procedures

The only procedure included in this static authoring set is documentary validation. Its checker PASS is structural; independent approval remains separate.

<a id="proc-001"></a>
### PROC-001 — Validate the four ownership documents

**Gatilho / resultado:** these four paths changed; five documentary checks pass.
**Modo / ambiente / permissão:** READ_ONLY, isolated worktree at assigned baseline; Python 3 with the frozen v1.3 checker. No Cargo/Rust.
**Entradas:** source pin, exact owned paths, checker `/tmp/corelink-ownership-v13-source/corelink-ownership-v1.3/tools/check_docs.py`.
**Pré-condições:** `HEAD` is the integration baseline; no unrelated staged or unstaged edits.

**review_status:** BLOCKED pending fresh independent review. **execution_status:** EXECUTED_LOCAL for this authoring pass. **required_for_acceptance:** true. **environment:** isolated worktree; checker from controlled v1.3 source bundle. **result:** PASS after M04 output is captured.

**review_evidence:** baseline/source pin comparison; four-path diff; R08 source inventory. **execution_evidence:** four checker JSON results and `git diff --check` output. **limitations:** no semantic approval, build, fuzz result, runtime reachability, or CI readback.

1. Run each S-profile check in M04 against the named path.
2. Run `git diff --check` after staging only the four owned paths.
3. Confirm staged path names equal the four-path allowlist and record outputs.

**Falhas e parada:** checker failure, extra path, missing source pin, or unclassified finding blocks completion. **Recuperação:** amend only owned documents, rerun all five checks, and preserve any unrelated user changes. **Certificação:** local documentary PASS only; independent cold review is still required. **Evidência:** exact baseline/pin, command output, staged path list, and resulting commit SHA.

[Procedure index](#m02)


<a id="m04"></a>
## M04 — Exact documentary validation matrix

Run the CO-1 v1.3 `check_docs.py` from the controlled extraction with `--root .`; its SHA-256 is `84d093bc9e95b79644fac56ca35e34b4e0ced9cd2b24ec1704485343236585b8`. Set `CHECKER=/tmp/corelink-ownership-v13-source/corelink-ownership-v1.3/tools/check_docs.py`. These four commands validate structure and S limits only. Add no Cargo or fuzz command to this ownership pass.

| Check | Exact selection | Required predicate | Current authoring state |
|---|---|---|---|
| Skill S | `python3 "$CHECKER" --kind skill --profile S --root . .claude/skills/own-corelink-client-verify-fuzz/SKILL.md` | `IMPLEMENTED_CHECKS_PASS` | EXECUTED_LOCAL; metrics returned with commit |
| Reference S | `python3 "$CHECKER" --kind reference --profile S --root . docs/ownership/crates/corelink-client-verify-fuzz/REFERENCE.md` | `IMPLEMENTED_CHECKS_PASS` | EXECUTED_LOCAL; metrics returned with commit |
| Blast-radius S | `python3 "$CHECKER" --kind blast_radius --profile S --root . docs/ownership/crates/corelink-client-verify-fuzz/BLAST_RADIUS.md` | `IMPLEMENTED_CHECKS_PASS` | EXECUTED_LOCAL; metrics returned with commit |
| Maintenance S | `python3 "$CHECKER" --kind maintenance --profile S --root . docs/ownership/crates/corelink-client-verify-fuzz/MAINTENANCE.md` | `IMPLEMENTED_CHECKS_PASS` | EXECUTED_LOCAL; metrics returned with commit |
| Whitespace | `git diff --check` | No whitespace errors | EXECUTED_LOCAL after staging the exact four paths |

`$CHECKER` names the pinned checker above, not an arbitrary executable. This matrix does not run or certify the workflow’s configured cargo-fuzz commands. If checkers fail, the artifact remains draft and blocked; do not claim PASS from intended commands.

<a id="m05"></a>
## M05 — Recovery and compatibility

| Surface | Reversibility | Safe recovery | Proof / limit |
|---|---|---|---|
| Four ownership documents | Tracked source is reversible by a scoped author commit | Amend/revert only this task’s four paths; preserve others’ edits | `git diff`, exact path list; no cold approval implied |
| Harness manifest/source | Tracked declaration and code are reversible | This pass makes no such change; a later owner must preserve the prior source and assess target compatibility | No compile or target run here |
| Fuzzer artifacts/corpus/cache | Unknown; not present in the four-file tracked subtree | Do not delete or reset generated/untracked findings; identify owner and retain evidence | This pass ran no fuzz or runner command |
| Parent Rust/FFI contract | Coordinated compatibility boundary | Route a separate change to the verified parent API owner; person/route currently UNKNOWN | No ABI/load or consumer compatibility test here |
| Workflow configuration or job state | Source edit may be revertible; external run/cache state is not proven reversible | No workflow or GitHub action is authorized by this manual | Static workflow source only; no run readback |

The fuzz package declares no persistent storage or production mutation. This is not evidence that a future runner leaves no artifacts or cache state.

<a id="m06"></a>
## M06 — Escalation and maintenance record

| Condition | Verified responsible route | Minimum evidence | Stop |
|---|---|---|---|
| Parent Rust/FFI contract decision | `corelink-client-verify` is the contract package; named person/team and route UNKNOWN | API/REL, source pin, requested decision | Do not assign an owner by inference |
| CI event, runner, corpus, or execution question | Workflow declares `corelink-builder`; operator/contact UNKNOWN | Workflow SHA, event/run ID supplied by authorized operator | No GitHub or runner access in this pass |
| Historical Merkle-fuzz security claim | Owning security/documentation authority UNKNOWN | Conflicting source lines from R08 and exact target files | Do not reuse as current fuzz evidence |
| Independent approval | Fresh reviewer not assigned | Four final file hashes and checker results | Author does not self-approve |

Completion record includes baseline, source pin and drift, exact four paths, checker metrics and outputs, commit SHA, unresolved relations, and explicit actions not run. Checker PASS is not a review verdict.

[Reference](REFERENCE.md#r01) · [Impact](BLAST_RADIUS.md#b01) · [Start](#m01)
