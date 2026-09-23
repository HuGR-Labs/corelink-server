---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-byok-fuzz
manifest: crates/corelink-byok/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w016-byok-fuzz-source-1177dad2
---

# corelink-byok-fuzz — maintenance

This manual covers bounded source review and document checks. It does not
authorize Cargo, Rust, fuzzing, workflow dispatch, provider, network,
production, database, storage, or deployment activity.

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) ·
[Test matrix](#m04) · [Recovery/compatibility](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Safe preparation

| Check | Expected predicate | Stop / recovery |
|---|---|---|
| Identity and baseline | Package is `corelink-byok-fuzz`; integration baseline `d80f245e0be3c61c250249de8292e42a6cd8ef5d`; source pin `1177dad2ca2a9f21c29b5a118aa7944b77147798` | Stop on mismatch; obtain a reconciled pin without resetting the worktree |
| Source scope | Root manifest and `crates/corelink-byok/` match between supplied baseline and source pin | Stop if scoped source drift appears; record changed paths and rescope |
| Host and permissions | Isolated checkout, read-only Git/source inspection | Stop if Cargo/Rust/build/fuzz or external operation is needed |
| Resource state | No target, corpus, or artifact execution in this static task | Preserve pre-existing local files; do not clean unknown artifacts |

<a id="m02"></a>
## M02 — Select a procedure

| Situation | Procedure | Mode | Effect allowed |
|---|---|---|---|
| Manifest, target, or runner source changed | [PROC-001](#proc-001) | `READ_ONLY` | Compare pinned source and exact target references |
| Prepare or review this four-file package documentation | [PROC-002](#proc-002) | `READ_ONLY` | Run the four supplied structural document checks and whitespace check |
| Request actual fuzz execution | [PROC-003](#proc-003) | `AUTHORIZED_OPERATION` | Block until an authorized environment and owner are identified |

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Reconcile source and static callers

**Trigger / expected result:** a target, manifest, script, or workflow changes; the pin and static caller set are recorded.

**Preconditions:** assigned isolated Git checkout; exact source revision known.

**Mode / environment / permission:** `READ_ONLY`; local Git object database; no network or package execution.

**Inputs:** manifest, two target paths, `scripts/fuzz-all.sh`, and `.github/workflows/fuzz-nightly.yml`.

**Steps:** (1) confirm `git rev-parse HEAD` is `d80f245e0be3c61c250249de8292e42a6cd8ef5d`; (2) inspect `git diff --name-status d80f245e0be3c61c250249de8292e42a6cd8ef5d 1177dad2ca2a9f21c29b5a118aa7944b77147798 -- Cargo.toml crates/corelink-byok`; (3) run `git grep -n -E 'corelink-byok-fuzz|wrapped_dek_parse|envelope_roundtrip' 1177dad2ca2a9f21c29b5a118aa7944b77147798 -- .`; (4) classify parent benchmark matches separately.

**Expected predicate:** both declared bins and each true static caller are reconciled; matches do not imply invocation.

**Failure / stop:** pin mismatch, unreadable source, or new owner boundary; stop and preserve output.

**Recovery:** obtain a new source packet; do not checkout/reset or infer an escalation route.

**Certification:** `review_status=BLOCKED`; `execution_status=EXECUTED_LOCAL`; mode `READ_ONLY`; environment: isolated author worktree; result `PASS`; `required_for_acceptance=true`.

**Review evidence:** pinned manifest, target sources, and caller paths; independent cold review remains pending.

**Execution evidence:** scoped baseline-to-pin diff was empty; exact identifier search found the two static caller files summarized in B02.

**Limitations:** no resolved Cargo graph, target run, or runtime evidence; broader inverse census remains UNKNOWN.

[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Validate the four ownership documents

**Trigger / expected result:** any of the four assigned artifact bytes change; each matching S-profile document checker and `git diff --check` pass.

**Preconditions:** exact four files exist; v1.3 candidate checker is supplied.

**Mode / environment / permission:** `READ_ONLY`; local Python checker and Git; no package command.

**Inputs:** skill, reference, blast radius, maintenance, checker root `.`.

**Steps:** run the four matching `check_docs.py` commands with `--profile S --root .`; run `git diff --cached --check` on the exact four staged paths.

**Failure / stop:** any checker error, extra path, or whitespace error; correct only an owned file and rerun affected checks.

**Recovery:** retain output, compare exact changed paths, and route cross-scope findings to integration.

**Certification:** `review_status=BLOCKED`; `execution_status=EXECUTED_LOCAL`; mode `READ_ONLY`; environment: isolated author worktree plus supplied v1.3 checker; result `PASS`; `required_for_acceptance=true`.

**Review evidence:** four assigned files and checker output; independent cold review remains pending.

**Execution evidence:** four S-profile checker outputs passed and `git diff --check` exited zero for this handoff.

**Limitations:** checker output is structural only; it does not approve meaning, completeness, or independence. The checker is supplied by integration, not tracked in this package.

[Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Request a fuzz run

**Trigger / expected result:** someone requests execution; an independently authorized isolated runner and bounded resource policy must first be identified.

**Preconditions:** neither is established in this source packet. The inspected workflow is a static configuration, not permission.

**Mode / environment / permission:** `AUTHORIZED_OPERATION`; execution blocked pending explicit operational ownership and authorization.

**Inputs:** target choice, toolchain, cached dependencies, timeout/memory limits, corpus destination, and result-retention owner.

**Steps:** do not launch a target in this procedure; obtain the missing owner/runner and a separate authorized execution packet.

**Expected predicate:** no run occurs until those facts are recorded. No route or person is named here.

**Failure / stop:** absent authorization, resource limit, or recovery owner; remain blocked.

**Recovery:** none by author; preserve the request and seek assignment through an externally defined process.

**Certification:** `review_status=BLOCKED`; `execution_status=BLOCKED_FOR_OPERATION`; mode `AUTHORIZED_OPERATION`; environment: no authorized runner identified; result `NOT_EXECUTED`; `required_for_acceptance=false`.

**Review evidence:** W016 static-only scope and dispatch-only workflow source; no operational approval evidenced.

**Execution evidence:** none.

**Limitations:** no target, CI, provider, or external-service execution evidence; owner, resource policy, and escalation route UNKNOWN.

[Procedure index](#m02)


<a id="m04"></a>
## M04 — Matrix of tests and validation

| Change / validation | Package / target / features | Command or procedure | Predicate | Environment / evidence |
|---|---|---|---|---|
| Parse-target intent | `corelink-byok-fuzz` / `wrapped_dek_parse` / selection UNKNOWN | PROC-003; workflow source names `cargo fuzz run` | No result claimed; execution not authorized here | Not executed; target declares `test=false`, `doc=false`, `bench=false` |
| Envelope assertion intent | `corelink-byok-fuzz` / `envelope_roundtrip` / selection UNKNOWN | PROC-003; workflow source names `cargo fuzz run` | No result claimed; execution not authorized here | Not executed; stub is source-local |
| Four ownership files | Four assigned paths / S | PROC-002 | Each supplied checker emits `IMPLEMENTED_CHECKS_PASS`; staged diff has no whitespace errors | Passed for this handoff; structural evidence only |

**Negative boundaries:** do not infer coverage, fuzz corpus quality, run duration,
provider activity, or a passed workflow from declarations or comments. No
`--all-features`, live suite, build, or deploy is selected by this manual.

<a id="m05"></a>
## M05 — Recovery and compatibility

| Surface | Reversible? | Existing data / compatibility | Safe response | Recovery proof |
|---|---|---|---|---|
| Harness source and these documents | Usually, if only the source patch is involved | No target run is evidenced; generated crash/corpus artifacts may exist elsewhere | Inspect exact diff and artifact ownership before reverting or removing anything | Restored source hash and clean scoped diff; do not claim for data |
| `WrappedDek` / `EncryptedBlob` wire shape in parent API | Not established by this package | N/N-1 compatibility and existing persisted values are UNKNOWN | Route to `corelink-byok` contract owner and the separately identified persistence/composition owner | Requires an independently approved compatibility check; these fuzz assertions are insufficient |
| Workflow-uploaded or local fuzz artifacts | Unknown | Workflow source describes a 14-day failure artifact retention; no artifact/run observed | Preserve; identify owner and retention state before cleanup | Artifact inventory/readback by its actual owner; not performed here |

Changing target assertions or rolling back the harness cannot restore external
envelope data. A source revert does not establish compatibility. The actual
composition root, persistence owner, and N/N-1 policy remain UNKNOWN; no
roll-forward, migration, or production recovery route is asserted.

<a id="m06"></a>
## M06 — Handoff and escalation

| Condition | Verified route | Evidence | Stop action |
|---|---|---|---|
| Parent envelope/API decision | `corelink-byok` implementation docs identify that package as API owner | [Parent reference](../corelink-byok/REFERENCE.md#r02) | Stop before changing parent code or claiming persisted compatibility |
| Workflow/runtime operation or independent review assignment | UNKNOWN | No named owner or route established in scoped source | Do not invent a person, channel, or permission; keep status blocked |

After a task, report baseline, source pin, exact changed paths, checker outputs,
affected API/INV/REL/PROC IDs, unexecuted operations, and all remaining unknowns.
Preserve files outside the four assigned artifact paths.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to preparation](#m01)
