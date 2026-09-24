---
schema: corelink-ownership/1.1
document: maintenance
package: migrate-single-to-multi-region
manifest: apps/migrate-single-to-multi-region/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w015-migrate-single-to-multi-region-static-20260921
---

# migrate-single-to-multi-region — maintenance

This manual is limited to static source inspection and local ownership-document validation. It does not authorize Cargo/Rust/tests/builds, a binary mode, network/provider access, GitHub, deployment, or any tenant, database, storage, Terraform, or production action. The manifest-advertised modes are not permission to run them.

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) ·
[Validation matrix](#m04) · [Recovery and compatibility](#m05) ·
[Escalation and handoff](#m06).

[Reference](REFERENCE.md#r01) · [Relations](BLAST_RADIUS.md#b01) · [Skill](../../../../.claude/skills/own-migrate-single-to-multi-region/SKILL.md#s01).

<a id="m01"></a>
## M01 — Safe preparation

| Item | Requirement |
|---|---|
| Source | Pin 1177dad2ca2a9f21c29b5a118aa7944b77147798; inspect apps/migrate-single-to-multi-region/Cargo.toml and src/main.rs. |
| Baseline | Integration baseline ab7137cd178f0e6cb282f3e944f9f7b58d0f5540 is separate from the source pin; compare package source, package manifest, and root Cargo.toml membership before updating claims. |
| Mode | READ_ONLY source review or LOCAL_ISOLATED document checker only. |
| Inputs | Exact requested change and selected target/source paths; never obtain tenant identifiers, credentials, provider state, or migration inputs for this task. |
| Stop | If the answer requires build, invocation, live region, audit delivery, data, provider, Terraform, recovery, or runtime evidence. |
| Recovery | Retain the bounded source claim and unknown; no state effect is created by static review. |

Five axioms: package.name is the identity; declared tests are not executed tests; imports are not runtime reachability; local fakes are not providers; advertised migration modes confer no execution authority. The canonical OKF concept route is the [Wave 015 plan](../../WAVE_015_PLAN.md), not duplicated here.

<a id="m02"></a>
## M02 — Choose a procedure

| Situation | Procedure | Mode | Effect allowed | Extra authorization |
|---|---|---|---|---|
| Parser, report, imported interface, or source-flow change | [PROC-001](#proc-001) | READ_ONLY | Read source and update assigned documents. | None for source inspection. |
| Ownership-document handoff | [PROC-002](#proc-002) | LOCAL_ISOLATED | Run the pinned checker at its exact path/hash on each document. | None; commands and hash are recorded in PROC-002. |
| Final hash, backlog, and cleanup handoff | [PROC-003](#proc-003) | LOCAL_ISOLATED | Capture exact bytes and report pending reconciliation; remove only task-local generated residue. | Integration lead must reconcile the canonical backlog and issue duplicates before publication. |
| Tenant migration, rollback, provider investigation, or request to run a mode | STOP — BLOCKED pending a verified operational owner and authorization route. | None | No binary or state action. | Wave 015 integration lead must obtain the owner, scoped approval, current-state evidence, and recovery plan before routing elsewhere; none is established here. |

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Review the source contract

**Objective / trigger:** a manifest, parser, mode, report, audit, or rollback-text change.

**Preconditions / inputs:** pinned source, assigned diff, and exact package paths; local checkout only.

**Mode / environment / permissions:** READ_ONLY; no Cargo, Rust, test, build, binary, network, provider, or production permission.

1. Compare the package manifest, main.rs, root workspace membership, and literal reverse/non-Cargo references with R01–R07 and B02–B05.
2. Trace changed values through Args::parse, main, the selected local function, imported contract, and any output; record what is still unknown.

**Expected predicate:** every changed source shape maps to an API/INV/REL and no fixture is described as provider behavior.

**Stop / failure:** a claim needs invocation, tenant data, runtime state, or an unverified owner; retain the gap and stop.

**Recovery:** revise only authorized docs, or request evidence through a verified owner; static review creates no state to restore.

**Review status:** REVIEWED. **Execution status:** EXECUTED_LOCAL. **Result:** PASS for the pinned static census only.

**Environment / evidence:** local read-only checkout; package Cargo.toml, src/main.rs, root Cargo.toml, Dockerfile, runbook, and named workflow/spec paths. **Execution evidence:** source and text inspection; no operation was run.

**Limitations:** no resolution, build, test, runtime, provider, data, or cold-review proof. **Required for acceptance:** true.

[Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Capture hashes and hand off for re-review

**Objective / trigger:** any of the four artifact bytes changes after review.

**Preconditions / inputs:** candidate commit, four paths, checker output, and campaign ledger.

**Mode / permissions:** LOCAL_ISOLATED documentation worktree; no Cargo, Rust, network, GitHub, provider, or production permission.

1. Run `git status --short`, `git diff --check`, and all four PROC-002 checker commands. 2. Capture `git hash-object` for every artifact and the candidate commit; record source pin `1177dad2ca2a9f21c29b5a118aa7944b77147798`. 3. Mark reviews of changed bytes stale; request a fresh independent cold verdict for each affected artifact. 4. Report Cargo.lock drift and the unknown operation owner separately. 5. Hand hashes and unknowns to integration; reconcile canonical backlog, aliases, and duplicate issues

before publication. 6. Remove task-created checker output after hashes; retain the candidate branch/worktree until integration or rejection.

**Expected predicate:** four passes, clean diff, exact hashes, stale-review records, and backlog handoff.

**Stop / failure:** extra path, hash mismatch, unrelated dirt, missing ledger entry, or premature publication.

**Recovery:** restore assigned bytes, rerun checks, recapture hashes, and request fresh review; Git revert is not data recovery.

**Review status:** BLOCKED pending independent cold review. **Execution status:** REVIEWED_NOT_EXECUTED; no handoff is claimed.

**Evidence:** command outputs, artifact hashes, candidate commit, source pin, paths, reviewer records, and backlog decision.

[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Validate and hand off the documents

**Objective / trigger:** finalize the four assigned files.

**Preconditions / inputs:** repository root, Python 3, four paths, and the checked-in candidate checker `docs/ownership/tools/check_docs.py` at the selected repository revision.

**Mode / environment / permissions:** LOCAL_ISOLATED; local Markdown only.

1. Run `python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-migrate-single-to-multi-region/SKILL.md`.
2. Run `python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/migrate-single-to-multi-region/REFERENCE.md`.
3. Run `python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/migrate-single-to-multi-region/BLAST_RADIUS.md`.
4. Run `python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/migrate-single-to-multi-region/MAINTENANCE.md`.
5. Run path-scoped `git diff --check` and `git status --short`; capture four `git hash-object` values for cold review. Campaign-wide scope is reconciled by integration.

**Expected predicate:** four passes and clean package-path whitespace; integration handles concurrent repository changes.

**Stop / failure:** checker error, source drift, or extra path; fix an assigned doc or report the blocker.

**Recovery:** keep failed output, correct the authorized doc, rerun affected final-byte checks.

**Review status:** REVIEWED. **Execution status:** EXECUTED_LOCAL. **Result:** PASS on this handoff.

**Environment / review evidence:** local repository; four JSON results and path/diff inspection. **Execution evidence:** checker and git diff only.

**Limitations:** structural checks do not establish semantics, runtime, operation, or cold approval. **Required for acceptance:** true.

[Procedure index](#m02)


<a id="m04"></a>
## M04 — Change-type validation matrix

These are documented future source checks; none was run in this ownership task. This package declares no custom package features. Its one target is the migrate-single-to-multi-region binary; dependency features declared locally are uuid v7 and serde.

A Cargo command does not invoke the program unless it explicitly runs the binary. No binary invocation is authorized here.

| Change type | Package / target / features | Command or procedure | Expected predicate | Environment / evidence |
|---|---|---|---|---|
| Parser, branch, local report source change | migrate-single-to-multi-region / bin migrate-single-to-multi-region / no package features | Future authorized: cargo check --locked --offline --package migrate-single-to-multi-region --bin migrate-single-to-multi-region. | Compile result only; it does not establish flag behavior or migration. | Local toolchain and offline cache would need separate confirmation; NOT RUN. |
| Report serialization or imported interface change | Same package/bin; uuid v7 and serde dependency features | Future authorized: cargo test --locked --offline --package migrate-single-to-multi-region --bin migrate-single-to-multi-region after a relevant test target exists. | Focused assertions cover the changed API/branch; current package source declares no tests. | No test source/result in this evidence set; NOT RUN. |
| Dependency declaration or target-feature change | Package manifest; one bin; no package features | Future authorized: cargo tree --locked --offline --manifest-path apps/migrate-single-to-multi-region/Cargo.toml --edges normal,build. | Declared/resolved edges agree for the selected offline environment. | Cache/target selection unknown; BLOCKED until separately authorized and recorded. |
| Cargo.lock drift or lockfile-only change | `Cargo.lock` package entry for migrate-single-to-multi-region | Compare the exact package entry at the pinned source and candidate baseline; resolve only in a separately authorized target-specific review. | This package's entry and dependency list are reconciled; unrelated lockfile changes are not attributed to this package. | Current baseline removes `regex` from unrelated `corelink-ops`; no resolution was run; DOCUMENTARY only. |
| Ownership documents | Four assigned artifacts; profile S | PROC-002 exact commands with pinned checker SHA and path-scoped `git diff --check` | Four IMPLEMENTED_CHECKS_PASS JSON verdicts and clean diff for these paths. Does not assert repository-wide exclusivity. | Local static checker; executed for this author handoff only. |
| Any dry-run, execute, rollback, D1, R2, or Terraform request | Binary mode; target and provider state unknown | STOP; no command or operation procedure. | No mode invocation or state touch occurs in this task. | Explicitly out of scope; no execution evidence is claimed. |

Negative checks for source review: contradictory mode flags must not be described as rejected unless parser code changes; dry-run fixture counts are not a D1 inventory; execute audit values are not computed hashes or durable events; rollback output is not restoration. Generic workspace CI is not a package-specific acceptance result.

<a id="m05"></a>
## M05 — Recovery and compatibility

| Surface | Reversible? | Existing state / compatibility | Safe action | Recovery proof |
|---|---|---|---|---|
| Assigned ownership documents | Yes, for those document bytes only | Final artifact hashes and any reviewer record become stale on edit. | Correct only the assigned files and rerun their structural checks; obtain fresh cold review. | Final hashes, clean path-scoped diff, and new review evidence. |
| Current inspected local binary source | Source edit is reversible in Git; external data effect is not shown by this source | No package-local persistent store or provider call is present in main.rs. This does not establish external data state. | Review and revert/roll forward only the exact code change; never infer a data rollback from Git. | Source diff and package-scoped validation when separately authorized. |
| Previously executed migration or provider state | Unknown; not established by these artifacts | D1 rows, R2 object versions, Terraform state, and N/N-1 compatibility are not inspectable here. | Stop. Use the verified operational owner and an approved recovery plan if one is later identified; the listed RB-region steps are not proof or authorization. | Must be supplied by that operation owner; none is available in this source-only record. |
| CLI and runbook contract | Conditional | RB-region documents flags and manual recovery text; actual callers/version skew are unknown. | Reconcile parser and all discovered invocation text before a separately approved release. | Static call-site/runbook review; no runtime compatibility result. |

The package source shows no persistent state of its own. The advertised migration can have irreversible external effects if implemented or carried out elsewhere; this document does not identify a composition-root recovery owner. Git revert cannot restore tenant rows, object versions, or infrastructure state.

<a id="m06"></a>
## M06 — Escalation, record, and done gate

| Condition | Verified responsibility / route | Minimum evidence | Prohibited action |
|---|---|---|---|
| Imported Region/report/audit contract change | corelink-region package contract; see its reference and maintenance docs | Exact changed symbol, consumer source path, and affected REL | Claiming provider execution from a type/interface edit. |
| Source-only ownership-document task | Wave 015 integration owner for this task; independent reviewer remains separate | Pin, baseline, four paths, checker JSON, diff and final hashes | Self-approval or editing generated registry/index files. |
| Artifact bytes changed after a review | Fresh independent reviewer for each changed artifact | Candidate commit, exact artifact hashes, stale prior-review records, and four new verdicts | Reusing an approval from an ancestor or unchanged-looking path. |
| Publication/backlog handoff | Campaign integration/publication owner | Canonical backlog search, aliases/manifest/marker duplicate decision, ledger update, and final readback | Publishing this package from this manual or treating a draft as an issue. |
| Author-worktree cleanup | Wave 015 integration lead after hash capture | Clean scoped status, retained candidate evidence, and list of removed task-local residue | Deleting an unintegrated branch/worktree or broad cleanup. |
| Live migration or rollback need | BLOCKED: actual operational owner and authorization route are UNKNOWN here; Wave 015 integration lead must route it before work proceeds | Independently verified identity, scoped approval, current state evidence, and approved recovery plan | Invoking any mode or touching tenant, database, storage, provider, Terraform, or production state under this manual. |

Current handoff records source pin 1177dad2ca2a9f21c29b5a118aa7944b77147798 and integration baseline ab7137cd178f0e6cb282f3e944f9f7b58d0f5540, manifest/source paths, Cargo.lock drift, changed paths, checker outcomes, skipped operations, and unknowns. Definition of done for author validation is four final artifacts, four S-profile structural passes, clean exact-path diff, exact artifact hashes, explicit cleanup/backlog handoff, and independent cold review pending. Any prior review of bytes changed in this candidate is stale; the author has not approved these artifacts.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Start](#m01)
