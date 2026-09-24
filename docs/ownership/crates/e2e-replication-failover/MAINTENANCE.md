---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-replication-failover
manifest: tests/e2e-replication-failover/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-replication-failover-static-20260921
---

# e2e-replication-failover — maintenance

This manual covers source-static documentation and safe handoff for the harness package. It does not authorize package execution, a drill, or an operational action.

[M01](#m01) · [M02](#m02) · [M03](#m03) · [M04](#m04) · [M05](#m05) · [M06](#m06).

<a id="m01"></a>
## M01 — Preparation and boundary

**Checkout / source:** integration baseline `527234c0a316e7f01615a7549e1797da355d69c5`; source evidence pin `1177dad2ca2a9f21c29b5a118aa7944b77147798`. Scoped root-manifest and package-source comparison is clean. **Mode:** `READ_ONLY` for source, `LOCAL_ISOLATED` for these docs. **Target facts:** library plus explicit `scenarios`; no feature declaration. **Stop:** any source/workspace drift, non-four-path diff, missing evidence, or request for execution/provider/runtime facts. Never execute a command merely because it appears in this manual.

<a id="m02"></a>
## M02 — Procedure selection

| Situation | Procedure | Mode | Expected result | Required here? |
|---|---|---|---|---|
| Refresh or verify package facts | [PROC-001](#proc-001) | `READ_ONLY` | Source pin and claims reconcile | Yes |
| Check final ownership documents | [PROC-002](#proc-002) | `LOCAL_ISOLATED` | Four S checks and whitespace check pass | Yes |
| Request concerns runtime/drill or production | Stop; no operation procedure is supplied | `AUTHORIZED_OPERATION` unavailable | Owner and authority remain unknown | Blocks operational claims |

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Refresh pinned package facts

**Trigger / expected result:** source/manifest change; claims map to the pin. **Mode / environment / permissions:** `READ_ONLY`, local checkout, no external access. **Inputs:** pin, baseline, root/package manifests and sources. **Preconditions:** confirm `HEAD`/pin; stop on drift.

1. Run `git diff --exit-code 1177dad2ca2a9f21c29b5a118aa7944b77147798 HEAD -- Cargo.toml tests/e2e-replication-failover` to detect source/manifest drift from the evidence pin. 2. Run `git diff --exit-code 1177dad2ca2a9f21c29b5a118aa7944b77147798 527234c0a316e7f01615a7549e1797da355d69c5 -- Cargo.toml tests/e2e-replication-failover` to compare the pinned source with the integration baseline. 3. Census: `git grep -n -I -e 'e2e-replication-failover' -e 'e2e_replication_failover' -- 'Cargo.toml' '**/Cargo.toml'`; root/nested covered; record static matches separately from graph output. 4. Census: `git grep -n -I -e 'E2EFixture' -e 'e2e_replication_failover' -e

'e2e-replication-failover' -- ':!docs/ownership/**'`; record workspace/package selectors; absence is not non-selection. 5. Retain line-anchored claims; selection, execution, operator, and runtime stay unknown without evidence.

**Stop / recovery:** stop on scoped diff; lead reconciles and removes unsupported claims.

**Review status:** `BLOCKED`; no independent cold review or reviewer identity.
**Execution status:** `EXECUTED_LOCAL`. **Result:** `PASS` for source inspection. **Evidence:** pinned/baseline diff clean; anchors: [reference](REFERENCE.md#r01).
**Required for acceptance:** true. **Limitations:** source only; no Cargo resolution, build, test, or runtime claim.

[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Validate the four documentary artifacts

**Trigger / expected result:** final bytes are ready; one structural check per assigned artifact and clean baseline whitespace diff. **Mode / environment / permissions:** `LOCAL_ISOLATED`, repository root, supplied candidate checker configured by `CHECKER`; local files only. **Inputs:** exact four paths and pinned source SHA. **Preconditions:** source facts are refreshed and no out-of-scope path is changed.

1. Run the skill, reference, blast-radius, and maintenance checker commands in [M04](#m04), each with `--root .` and profile `S`.
2. Run `GIT-001` and `GIT-002` from [M04](#m04); verify `GIT-003` lists only the four assigned paths.
3. Preserve each JSON verdict/metrics and diff/scope result in the handoff; structural success is not semantic approval.

**Stop / recovery:** on any failure, repair an owned artifact only, then rerun all four checks; scope drift is handed to the lead.

**Review status:** `BLOCKED`. **Review evidence:** no independent cold review has been received; reviewer identity is absent.
**Execution status:** `EXECUTED_LOCAL`. **Result:** `PASS` only if each stated predicate passes. **Execution evidence:** four checker JSON results, scoped `git diff --check`, and the exact path list, preserved in the author handoff.
**Required for acceptance:** true. **Limitations:** structural checks do not verify meaning, Cargo selection, test execution, or runtime.

[Procedure index](#m02)


<a id="m04"></a>
## M04 — Change-type validation matrix

Set `CHECKER` to the supplied v1.3 candidate's `tools/check_docs.py` path. The path is an execution input, not a permanent repository path.

| Change type | Package / target / features | Command or procedure | Environment and expected predicate |
|---|---|---|---|
| Skill document | Package docs; skill artifact; features N/A | `python3 "$CHECKER" --kind skill --profile S --root . .claude/skills/own-e2e-replication-failover/SKILL.md` | Local checker dependencies installed; JSON `IMPLEMENTED_CHECKS_PASS` and S metrics; structural only |
| Reference document | Package docs; reference artifact; features N/A | `python3 "$CHECKER" --kind reference --profile S --root . docs/ownership/crates/e2e-replication-failover/REFERENCE.md` | Same environment; JSON `IMPLEMENTED_CHECKS_PASS` and S metrics; structural only |
| Blast-radius document | Package docs; blast-radius artifact; features N/A | `python3 "$CHECKER" --kind blast_radius --profile S --root . docs/ownership/crates/e2e-replication-failover/BLAST_RADIUS.md` | Same environment; JSON `IMPLEMENTED_CHECKS_PASS` and S metrics; structural only |
| Maintenance document | Package docs; maintenance artifact; features N/A | `python3 "$CHECKER" --kind maintenance --profile S --root . docs/ownership/crates/e2e-replication-failover/MAINTENANCE.md` | Same environment; JSON `IMPLEMENTED_CHECKS_PASS` and S metrics; structural only |
| Library source change | `e2e-replication-failover`; `--lib`; no features declared | `cargo test --locked --offline -p e2e-replication-failover --lib` | Local offline checkout/cache; compile and library tests pass; **not run in this documentation task** |
| Scenario source change | `e2e-replication-failover`; `--test scenarios`; no features declared | `cargo test --locked --offline -p e2e-replication-failover --test scenarios` | Local offline checkout/cache; all declared scenario assertions pass; **not run in this documentation task** |
| Manifest/workspace dependency change | Package metadata and declared targets; features unresolved | `cargo metadata --locked --offline --no-deps --format-version=1` | Local offline checkout; inspect emitted package/member/target/dependency declarations; does not resolve reachability or execute targets; **not run here** |
| Inverse dependency census | Workspace package graph; default target and declared features | `cargo tree --workspace --locked --offline --invert e2e-replication-failover`; repeat with `--invert corelink-replication-coordinator` and `--invert corelink-replica-worker` | Local offline checkout/cache; record exact inverse package nodes; **not run here**; this is graph evidence, not deployment reach |
| CI/build/deploy selection | Workflow, script, release and deploy files; package target unknown | `git grep -n -I -e 'e2e-replication-failover' -e 'cargo test --workspace' -e 'cargo nextest run --workspace' -- .github scripts releases deploy* 2>/dev/null` | Static command census only; preserve workflow path/line and state actual dispatch/selection as unknown; **not a CI run** |
| Final source/document diff | Four assigned docs against source pin and integration baseline | `GIT-001`–`GIT-003` below | Local repository; no whitespace errors in owned paths and exactly four assigned paths against integration baseline |

`GIT-001` scopes whitespace checking to the authored files because unrelated source/integration tree differences exist outside this assignment. `GIT-003` checks scope against the required integration baseline.

```sh
git diff --check 1177dad2ca2a9f21c29b5a118aa7944b77147798 -- \
  .claude/skills/own-e2e-replication-failover/SKILL.md \
  docs/ownership/crates/e2e-replication-failover/REFERENCE.md \
  docs/ownership/crates/e2e-replication-failover/BLAST_RADIUS.md \
  docs/ownership/crates/e2e-replication-failover/MAINTENANCE.md
git diff --check 527234c0a316e7f01615a7549e1797da355d69c5
git diff --name-only 527234c0a316e7f01615a7549e1797da355d69c5
```

**Negative cases:** do not infer execution from a declared target or historical audit row; do not treat the in-memory audit snapshot as delivery; do not call `--all-features`, live/ignored tests, providers, network, CI/GitHub, deployment, production, database, or storage. The four structural checks do not verify scenario predicates or approve content.

<a id="m05"></a>
## M05 — Compatibility and recovery

| Surface | Reversibility / existing state | Safe recovery | Proof boundary |
|---|---|---|---|
| This documentation set | Git-tracked text; no external state created by these edits | Restore only the affected owned file to its prior committed/reviewed bytes; with no reviewed version, keep approval blocked until a fresh cold review | `git diff --check` proves whitespace only; cold review remains separate |
| Fixture/scenario source | Fixture uses in-memory coordinator, registry, and sink; test inputs are local values | Revert the package-local source change after checking its exact diff; run the selected package target only when separately authorized | Revert does not prove a test or provider recovery; external consumers are unknown |
| Manifest/workspace wiring | Declaration changes can alter future package selection or resolution | Restore the exact prior declaration after reviewing its impact; do not reset unrelated workspace/lockfile changes | Cargo graph/CI selection not established here |
| Drill/runtime state | Production composition root, storage, audit transport, and operator are unknown | No recovery command is provided; stop and obtain the verified operational owner and runbook before any action | `E2EFixture::new` is only the local test composition root; Git revert cannot reverse external effects |

Compatibility: `publish = false` and one in-repository scenario import are statically visible, but a complete consumer graph and external compatibility promise are unknown. Do not claim this helper has no consumers beyond the searched source paths.

<a id="m06"></a>
## M06 — Evidence, review, and escalation

| Condition | Verified route | Evidence to retain | Stop |
|---|---|---|---|
| Coordinator semantics/API | `corelink-replication-coordinator` is the source owner; human escalation identity/route unknown | Source SHA, symbol, affected REL and consumer predicate | Do not invent a person, team, or operational route |
| `Region` contract | `corelink-replica-worker` is the defining owner; human escalation route unknown | Worker definition, coordinator re-export, scenario import | Keep type implementation ownership upstream |
| Inverse graph / peer REL reconciliation | Campaign lead and the peer package ownership records; no package-local maintainer is inferred | Exact command, output, affected shared relation key, candidate SHA | Stop before closing blast coverage when inverse output or peer facts conflict |
| CI/build/deploy selection | Workflow/release owner is not identified by the pinned source search | Workflow path/line, exact command, branch/target context, result class | Do not claim selection, artifact publication, or deploy reach from a source search |
| Backlog / ownership issue | Campaign publication ledger and canonical backlog are lead-owned; this package task does not edit them | Package name, manifest, candidate SHA, changed blobs, review state, duplicate search decision | Do not create or mutate an issue/ledger entry from this manual; hand the exact packet to the lead |
| Temporary files / worktree | Author and lead own task-created checkout and temporary paths | Predicate: `git status --porcelain=v1` is empty after handoff; `git worktree list --porcelain` has no abandoned task worktree; `find /tmp -maxdepth 1 -name 'corelink-ownership-w015-e2e-replication-failover*' -print` is reconciled | Retain command output and deletion/reconciliation result; stop before removing an unverified or user-owned path |
| Runbook, drill, staging, production or audit delivery | Unknown | No operational evidence was gathered | No operation or package approval on this basis |
| Cold approval | Lead/reviewer responsibility, separate from authorship; reviewer identity not supplied | Four final artifact hashes, four verdicts, findings and context-new evidence | No author self-approval; any byte change invalidates affected review |

After maintenance, hand off source pin, baseline, exact changed paths, checker outputs, diff/scope result, cleanup command output, pending peer-side REL reconciliation, and operator/reviewer routes. The lead must reconcile this candidate against the campaign publication ledger and canonical backlog using the stable package marker before any issue write; a stale/duplicate/unknown ledger result keeps publication blocked. Do not edit registry/index/backlog in this package task.

[Reference](REFERENCE.md#r01) · [Relations](BLAST_RADIUS.md#b01) · [Start](#m01).
