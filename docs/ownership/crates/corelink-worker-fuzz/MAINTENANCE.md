---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-worker-fuzz
manifest: crates/corelink-worker/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: worker-fuzz-source-20260921
---

# corelink-worker-fuzz — maintenance

[Preparation](#m01) · [Choose a procedure](#m02) · [Procedures](#m03) ·
[Validation matrix](#m04) · [Recovery](#m05) · [Escalation](#m06).

<a id="m01"></a>
## M01 — Safe preparation

Use the exact source pin and isolated inputs before any future package operation. This document's authoring scope is static documentation only; configured workflows are not run evidence.

| Preparation item | Verified value or limit |
|---|---|
| Integration baseline / source pin | `8cdc02828132b9b6f03a3b57117b8140325f6762` / `1177dad2ca2a9f21c29b5a118aa7944b77147798`; ignore divergent local `main`, do not fetch or switch branches |
| Toolchain / target | Independent cargo-fuzz workspace; CI selects self-hosted Mac nightly and cargo-fuzz 0.13.1; installed local versions and host support unknown |
| Corpus / resource bound | Corpus state not inventoried; body copied before 5 MiB limit; isolated worktree and explicit `-max_len` required for a future authorized run |
| CI artifact state | Root nightly matrix is the active 3,600s schedule/dispatch surface; its self-hosted job skips the github-hosted-only rust cache and uploads failure artifacts for 14 days; corpus persistence is unknown; per-crate nightly is dormant |
| This review | No build, test, fuzz run, provider call, database/storage operation, or deployment |

<a id="m02"></a>
## M02 — Procedure selection

| Situation | Procedure | Mode | Effect permitted | Extra authorization |
|---|---|---|---|---|
| Check or revise these four ownership documents | [PROC-001](#proc-001) | Read-only local Python | Reads documents and emits structural metrics | No, within this documentation task |
| Change a fuzz target or its direct API usage | [PROC-002](#proc-002) | Isolated local fuzz run | Builds/runs fuzz binary and writes task-owned corpus/artifact output | Confirm host/resource budget and package owner first |
| Triage a crash, timeout, or corpus item | [PROC-003](#proc-003) | Isolated local review; rerun only under PROC-002 | Preserves/reviews task-owned finding data | Verify data handling and reviewer before sharing/rerun |

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Run the four S-profile document checks

**Trigger / result:** a byte change to a package artifact; each source file passes the v1.3 structural check under profile S.
**Mode / environment:** `LOCAL_ISOLATED`; Python and the checked-in candidate checker in this repository. This validates document structure only.
**Inputs / preconditions:** exact four package paths exist; Python dependencies for the provided checker are already available; no install or network step.

1. Run `python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-worker-fuzz/REFERENCE.md`.
2. Repeat with `BLAST_RADIUS.md --kind blast_radius`, `MAINTENANCE.md --kind maintenance`, and `.claude/skills/own-corelink-worker-fuzz/SKILL.md --kind skill`, retaining each JSON result and its metrics.
3. Confirm all four results have `IMPLEMENTED_CHECKS_PASS`; this is structural validation, not semantic completeness or cold approval.

**Stop / recovery:** stop if the checked-in checker or Python is unavailable. Correct only the affected package document and rerun all four. **Review status:** REVIEWED. **Execution status:** REVIEWED_NOT_EXECUTED until actual output is recorded. **Evidence:** exact paths, checker JSON, and final hashes outside documents.

[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Run a bounded local fuzz smoke

**Trigger / result:** an authorized target or dependency change; the selected harness completes within its recorded wall-time and input-size bounds without an untriaged finding.
**Mode / environment:** isolated local fuzz run; creates build output and corpus/artifact files. No credentials, provider, or production access.
**Preconditions:** verified source revision; reviewer-approved host resource budget; disposable worktree; task-owned corpus and artifact paths; compatible already-installed nightly/cargo-fuzz toolchain.

1. From `crates/corelink-worker`, run `cargo fuzz run r2_path -- -max_total_time=60 -max_len=1048576` for the key oracle.
2. Run `cargo fuzz run r2_put_get_roundtrip -- -max_total_time=60 -max_len=1048576` for the roundtrip oracle.
3. Record source/host/tool versions, flags, corpus inputs, elapsed time, exit status, and any artifact; stop on the first crash or resource-limit signal.

**Limit:** these 1 MiB smoke bounds cannot reach the target's body-over-5-MiB branch. That branch needs a separately reviewed input and memory budget because body allocation precedes the cap check; this procedure does not certify it. **Recovery:** preserve the finding and task-owned corpus, then use PROC-003. **Certification:** not executed in this documentation task; future required status depends on the changed code. **Evidence:** exact run record; CI YAML alone is insufficient.

[Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Triage and preserve a fuzz finding

**Trigger / result:** a crash, assertion, timeout, or resource finding is classified and reproducible status is stated without losing the original bytes.
**Mode / environment:** local review of one task-owned input/artifact; reproduction is a separate PROC-002 run.
**Preconditions:** exact target, source pin, and artifact origin are known; independent reviewer and data-sharing route are verified before export.

1. Copy the original input and metadata into a task-owned evidence directory; compute and record a content hash without editing the original. 2. Match the finding to the exact target assertion or failure branch and record whether the observed backend is `InMemoryR2`. 3. Do not add it to a shared corpus or upload it until reviewed for sensitive data and accepted by the verified owner. 4. If source changes, retain

the original, create a separate minimized copy only with an authorized local tool, then reproduce under PROC-002.

**Stop / recovery:** unknown provenance, possible customer/secret data, or no reviewer means quarantine and stop; do not discard or publish. Revert only task-owned code/corpus edits, not shared cache or another author's files. **Certification:** review-only here; no finding was generated or reproduced. **Evidence:** input hash, source/target pin, classification, reviewer decision, and separate execution record if run.

[Procedure index](#m02)


<a id="m04"></a>
## M04 — Validation matrix

| Change type | Package / target / selection | Procedure | Required predicate | Current evidence |
|---|---|---|---|---|
| Ownership document edit | Four canonical files / profile S | PROC-001 | All four structural results pass; limits and links remain valid | To be reported after this authoring check; no semantic approval implied |
| Key/prefix oracle edit | `r2_path` / standalone fuzz workspace | PROC-002 | Assertions hold for generated inputs; crashes investigated | Not run in this documentation scope |
| Read/write/isolation oracle edit | `r2_put_get_roundtrip` / standalone fuzz workspace | PROC-002 | Fresh, Duplicate, byte equality, cross-tenant NotFound; size branch separately planned | Not run; 1 MiB procedure does not reach oversize branch |
| Dependency, workspace, or feature edit | Both bins and all three path dependencies | Manifest review then authorized package gate | Target and resolved feature selection match intended source | Cargo graph not resolved here |
| CI trigger, time, runner, or cache edit | PR smoke, dormant per-crate nightly declaration, and active root nightly matrix | Workflow review by verified operator; actual run is separate | Preserve 60s PR smoke, 3,600s root matrix, intentional trigger ownership, corpus/artifact retention, and run result | YAML inspected; no workflow was invoked |

**Negative cases:** short inputs return early (`<49` and `<65` bytes); cross-tenant check is conditional on UUID inequality; oversize path is specific to the roundtrip target and occurs after allocation. Test these only with a selected, resource-bounded gate.

**Shared gates:** no package-local ownership checker is present in the pinned repository; the v1.3 structural checker is external to this checkout. Existing CI definitions are coordination evidence, not an instruction to start jobs. Do not run all features, build, tests, fuzz, or deploy for a documentation-only change.

<a id="m05"></a>
## M05 — Compatibility and recovery

| Surface | Reversible? | Existing state / condition | Safe action | Recovery proof |
|---|---|---|---|---|
| Harness source or assertion | Usually by scoped source revert | A changed oracle can hide a finding or disagree with worker contract | Restore only the task-owned change; retain original finding input | Diff against baseline and independent oracle review |
| Local corpus / crash artifact | Do not assume | Fuzzer writes outside source; provenance may be unknown | Preserve task-owned originals; quarantine sensitive/unknown data; remove only outputs created by this task after review | Hash and inventory before/after; no shared cleanup |
| CI warm corpus / artifacts | Not by source revert | Root matrix uploads failure artifacts for 14 days; no corpus persistence is established by the workflow | Ask verified workflow operator to recover/expire only the affected run artifact; treat corpus state as unknown | Run ID and storage-state readback from that operator |
| Actual R2 object or database state | Not applicable to observed harness path | Target uses only `InMemoryR2`; a future real binding would be a new composition boundary | Stop and assign storage/composition owner before changing it | No durable remote state is evidenced by current source |
| Workflow schedule or runner | Partial | Per-crate `on.schedule` is commented; root `nightly.yml` remains schedule/dispatch-capable; already-running jobs/resources are not undone by reverting YAML | Review root schedule before changing per-crate YAML; coordinate with verified workflow operator and preserve run identifiers | Workflow revisions plus run-specific outcome; no duplicate schedule claim |

No current target state is shown as durable application data. Git revert cannot recover lost corpus/artifact data or cancel a running CI job.

<a id="m06"></a>
## M06 — Escalation and maintenance record

| Condition | Verified route | Minimum evidence | Do not do |
|---|---|---|---|
| Parent API or storage semantics are unclear | Default CODEOWNERS route requests `@gmhelmold`; independent contract owner is not identified | API/REL, source pin, concrete mismatch | Treat routing as contract ownership or choose parent storage policy |
| Fuzz runner budget or CI artifact needs operation | CODEOWNERS routes workflow files to `@gmhelmold`; operator identity and authority are unknown | Workflow path, run ID, resource/artifact scope | Start a provider/GitHub job or clear shared corpus/cache |
| Finding may contain sensitive input | CODEOWNERS requests review, but its header says it is not a control and the sole owner is usually the author; independent data-review route is unknown | Hash, origin, affected target, no raw data in issue text | Upload, share, or delete original bytes |
| Artifact ready for acceptance | Four independent cold reviewers are not assigned here | Four final artifact hashes and source pins | Self-approve or label source inspection as runtime evidence |

After authorized work, record baseline and actual result, preserve review evidence, clean only task-created temporaries, reconcile impacted RELs, and submit final hashes for independent review.

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
