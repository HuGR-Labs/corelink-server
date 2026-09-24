---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-tenant-path-fuzz
manifest: crates/tenant-path/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w016-tenant-path-fuzz-source-20260921
---

# corelink-tenant-path-fuzz — maintenance

Static/source-only procedures for this ownership authoring wave. None authorizes Cargo, Rust, test, build, fuzz, network, GitHub, provider, database, storage, deployment, publication, or production activity.

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) · [Test matrix](#m04) · [Compatibility and recovery](#m05) · [Escalation and record](#m06)

<a id="m01"></a>
## M01 — Safe preparation

The integration baseline is `8cdc02828132b9b6f03a3b57117b8140325f6762`; source pin is `1177dad2ca2a9f21c29b5a118aa7944b77147798`. Use an isolated checkout and inspect only tracked source and documentation. The pinned commit object and package scope must be available locally; missing source or drift stops review. Never load service secrets or use real tenant keys as fuzz inputs.

The procedures below are READ_ONLY and describe checks, not actions already performed. Fuzz target declarations and workflow selectors are not evidence of execution. Preserve any pre-existing worktree changes or crash artifacts; do not clean ignored `corpus` or `artifacts` paths to make the tree appear clean.

<a id="m02"></a>
## M02 — Select a procedure

| Situation | Procedure | Mode | Required evidence |
|---|---|---|---|
| Verify source pin and owned path scope | [PROC-001](#proc-001) | READ_ONLY | Git diff and path list |
| Review target byte layout or assertion change | [PROC-002](#proc-002) | READ_ONLY | Target source and M04 predicate comparison |
| Triage an existing minimized input or crash artifact | [PROC-003](#proc-003) | READ_ONLY | Artifact identity, target, classification, and parent boundary |
| Review CI target wiring | [PROC-004](#proc-004) | READ_ONLY | Workflow triggers, target names, and run evidence if separately provided |

Review status for all four procedures: author source review only; independent cold
review is pending. During this authoring, PROC-001, PROC-002, and PROC-004 were
`EXECUTED_LOCAL` as read-only source checks. PROC-003 is `REVIEWED_NOT_EXECUTED`
because no finding artifact was supplied. These states do not certify fuzzing or
CI. PROC-001/002/004 are required for their source claims; PROC-003 is not required
for this documentation-only scope because its trigger is absent, but it is required
before closing any future finding.

<a id="m03"></a>
## M03 — Procedures

Each procedure is READ_ONLY. None runs Rust or fuzzing.

<a id="proc-001"></a>
### PROC-001 — Verify pinned source and scope
**Trigger / expected result:** authoring or reviewing these artifacts; both
baselines and package-source provenance match.
**Mode / environment / permission:** READ_ONLY in isolated local Git checkout;
no network.
**Inputs / validation:** both pinned SHAs, the package path, and a local diff.
**Preconditions:** commit objects are locally available; checkout is the supplied
integration baseline.
1. Confirm the integration checkout is at the supplied baseline.
2. Confirm the source-pin commit object exists locally; compare only the fuzz package tree to it.
3. Record the exact changed paths and inspect the target manifest/source.
```sh
test "$(git rev-parse HEAD)" = "8cdc02828132b9b6f03a3b57117b8140325f6762"
git cat-file -e 1177dad2ca2a9f21c29b5a118aa7944b77147798^{commit}
git diff --check 1177dad2ca2a9f21c29b5a118aa7944b77147798 -- Cargo.toml crates/tenant-path/fuzz
git diff --name-status 1177dad2ca2a9f21c29b5a118aa7944b77147798 -- Cargo.toml crates/tenant-path/fuzz
git status --short --untracked-files=all
```
**Expected predicate:** package tree matches the source pin, or every difference
is stated.
**Failure / stop:** missing commit or unexplained drift.
**Recovery:** restart from the supplied isolated baseline; do not fetch or rewrite
history.
**Certification:** author source review only, cold review pending; execution
`EXECUTED_LOCAL`; `required_for_acceptance=true`.
**Evidence / result / limit:** `HEAD=8cdc02828132b9b6f03a3b57117b8140325f6762`;
`git diff --name-status` reports no root-manifest or fuzz-package drift. No Cargo
or fuzz action.
[M02](#m02)

<a id="proc-002"></a>

### PROC-002 — Review harness predicates
**Trigger / expected result:** target input, assertion, or comment changes; the
documented matrix matches source.
**Mode / environment / permission:** READ_ONLY local source review.
**Inputs / validation:** both target files, `REFERENCE.md#r05`, and M04.
**Preconditions:** source pin confirmed by PROC-001.
1. Trace the minimum length and every byte slice in each closure.
2. List each executable assertion separately from comments and intended properties.
3. Compare parent API signatures; update API/INV/REL records if a contract changed.
4. Reconcile changes with M04 and M05 before finalizing docs.
```sh
git show 1177dad2ca2a9f21c29b5a118aa7944b77147798:crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs
git show 1177dad2ca2a9f21c29b5a118aa7944b77147798:crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs
```
**Expected predicate:** every threshold, offset, assertion, and limitation has
a source path.
**Failure / stop:** ambiguity or an unsupported claim; narrow it or mark UNKNOWN.
**Recovery:** correct the record against pinned source.
**Certification:** author source review only, cold review pending; execution
`EXECUTED_LOCAL`; `required_for_acceptance=true`.
**Evidence / result / limit:** target files were inspected and mapped to M04;
no fuzz run or build.
[M02](#m02)

<a id="proc-003"></a>

### PROC-003 — Triage an existing finding
**Trigger / expected result:** a minimized input or crash artifact is supplied;
classify its target and failure layer without rerunning it.
**Mode / environment / permission:** READ_ONLY; synthetic artifact only.
**Inputs / validation:** artifact hash/path, target name, reported toolchain/run
provenance, and parent source.
**Preconditions:** artifact is safe to inspect and provenance is provided.
1. Preserve the original artifact and record its identity before review.
2. Map its bytes to the target slices; distinguish harness assertion from parent-library behavior.
3. Route a library defect to `corelink-tenant-path`; do not assign a person without evidence.
4. Identify a parent regression/vector check that would independently represent the defect.
**Expected predicate:** target, bytes, assertion, and owner boundary are explicit.
**Failure / stop:** artifact is missing, sensitive, or lacks provenance;
quarantine it and request safe evidence.
**Recovery:** do not reproduce sensitive bytes; use a verified security/privacy
route.
**Certification:** author review only; execution `REVIEWED_NOT_EXECUTED`;
`required_for_acceptance=false` here because no finding was supplied.
**Evidence / result / limit:** none; trigger absent. A future finding cannot
close without this procedure.
[M02](#m02)

<a id="proc-004"></a>

### PROC-004 — Review configured workflow coverage
**Trigger / expected result:** workflow target or trigger changes; configuration
and run evidence remain distinct.
**Mode / environment / permission:** READ_ONLY pinned repository-source review;
no GitHub query.
**Inputs / validation:** tenant-path, nightly, and fuzz-nightly workflows;
aggregate selector; optional run record.
**Preconditions:** source pin confirmed by PROC-001.
1. Record each trigger, target, working directory, and time bound from source.
2. Mark configuration `wired`; leave `runtime_verified=unknown` until a run record is inspected.
3. Confirm whether the target is covered by PR, scheduled, or dispatch-only paths.
```sh
git grep -n -E 'derive_prefix(_extended)?|cargo fuzz run' 1177dad2ca2a9f21c29b5a118aa7944b77147798 -- .github/workflows/tenant-path.yml .github/workflows/nightly.yml .github/workflows/fuzz-nightly.yml scripts/fuzz-all.sh
```
**Expected predicate:** target-to-trigger mapping matches source; no green status
inferred.
**Failure / stop:** trigger is absent, schedule is parked, run output is absent,
or runner/toolchain differs; record the gap.
**Recovery:** request an authorized run record from a verified operator.
**Certification:** author source review only, cold review pending; execution
`EXECUTED_LOCAL` for configuration; `required_for_acceptance=true` for source claims.
**Evidence / result / limit:** workflows inspected; no run ID/status was queried;
`runtime_verified=unknown`.
[M02](#m02)


<a id="m04"></a>
## M04 — Exact target and validation matrix

| Change / surface | Package / target / features | Command or PROC | Predicate | Environment / evidence |
|---|---|---|---|---|
| Short base input | `corelink-tenant-path-fuzz::derive_prefix`; no feature claim | PROC-002; no runtime command | `<48` returns before API call | Source only; no fuzz result |
| Accepted base input | Same target; raw bytes, declared UUID `v7` feature | PROC-002; PR 60s / nightly 3600s are config only | `[0..32]` TDK, `[32..48]` UUID; length 16 and URL-safe bytes; Display/Debug called | Workflow source; no run inspected |
| Short extended input | `derive_prefix_extended`; feature selection unresolved | PROC-002 | `<80` returns before API call | Source only; no result |
| Accepted extended input | Same target; raw TDK A/UUID/TDK B slices | PROC-002; dispatch matrix declares 1800s | Repeated A/same UUID equality and first output shape; changed inputs length only | Dispatch-only/parked schedule; no run |
| Changed UUID/key paths | Extended target; one bit `data[0] & 0x7f` | PROC-002 | No injectivity, key separation, or full 128-bit sweep assertion | Limitation is source evidence |
| Aggregate selector | `scripts/fuzz-all.sh`; target names only | PROC-004; invocation unknown | Both names are listed; list is not execution | Script source only |
| Parent/parity gates | Parent crate tests and Rust/Worker vectors | Outside this package; future authorized selection | Fuzzer does not validate TypeScript or stored data | Consumer and runtime evidence absent |

All rows are `SOURCE` configuration or predicate facts. No local validation, fuzzing, cargo-fuzz run, CI query, or cross-language test was executed by this author.

<a id="m05"></a>
## M05 — Compatibility and recovery

| Surface | Reversibility and risk | Safe response and evidence |
|---|---|---|
| Harness byte layout / thresholds | Reversible in source; old corpus bytes may mean different fields or fall below a new threshold | Preserve minimized inputs, record old/new offsets, and decide whether to retain or reclassify corpus entries |
| Harness assertion/comments | Reversible, but weakening a predicate can hide a previously detectable class | Compare executable predicate to the intended property; retain the prior failing artifact and add an independently selected regression gate |
| Parent prefix algorithm, output width, or UUID interpretation | May change derived storage namespace and cross-language compatibility; git revert alone does not recover objects | Coordinate with parent API and storage/data owner; prove old/new reading or migration with synthetic fixtures before an authorized operation. Owner is UNKNOWN here |
| Workflow trigger, target, or runner | Reversible in YAML; can remove or change future coverage | Restore intended target mapping and preserve run records; no success claim without actual run evidence |
| Fuzz corpus/artifacts | Files are ignored by Git; deletion can remove reproducer evidence | Preserve and hash relevant artifacts before cleanup; never treat corpus absence as proof of no finding |

This fuzz package has no source-visible database, bucket, cache, migration, or runtime writer. It cannot perform data recovery. Prefix compatibility and any stored-object recovery plan belong to the relevant integration/storage owner, not the harness author; the person/team is unverified.

<a id="m06"></a>
## M06 — Escalation and record

| Condition | Route | Record | Avoid |
|---|---|---|---|
| Harness contract changes | Package docs and independent reviewer; reviewer not yet assigned | Source pin, target, API/INV/REL/PROC IDs | Self-approval |
| Parent algorithm or output changes | `corelink-tenant-path` boundary; named person unknown | Parent diff, cross-package manifests, parity vectors, compatibility decision | Treating fuzz output as migration proof |
| Configured workflow lacks run evidence | Authorized CI operator; identity unknown | Workflow source plus actual run identifier/status when supplied | Inferring green from YAML |
| Secret/customer bytes appear in artifact | Security/privacy owner not verified | Quarantine reference and safe artifact hash only | Reproducing or copying sensitive input |
| Source owner or escalation route is requested | **BLOCKED: no verified source-owner UID, team, or escalation route appears in the pinned evidence** | Record the missing attribution and keep the request at the independent review gate; do not name a person | Inventing an owner, queue, approval, or runtime route |

After any procedure, record the exact source pin, integration baseline, artifact
paths, and real result; clean only temporary files created by the task. Reconcile
the four final document paths and their hashes before cold review. Route changed
parent/API/REL/INV claims to the parent package review; route storage or parity
claims to the verified integration owner. No owner is named here, and no hash or
approval is invented.

The source-owner/escalation gap is an explicit acceptance blocker: a later review
may proceed only after repository evidence supplies a verified attribution or
route; until then, parent, storage, CI, security, and runtime requests remain
unassigned and UNKNOWN.

The five acceptance axioms are:

- **Success criteria:** a maintainer can route fuzz-package work, distinguish target intent from execution, find the package boundary, and choose safe static maintenance actions.
- **Completeness criteria:** package identity, targets, features, direct dependencies, exact harness calls and assumptions, inverse consumers, source/re-export/workflow/build boundaries, failures, and explicit unknowns are reconciled without inferring coverage from placement.
- **Quality standards:** use the exact v1.3 inputs, atomic directed relations, falsifiable invariants, stable navigation, and source evidence. Distinguish implementation, public contract, composition root, runtime operator, and review authority; leave unverified escalation blocked.
- **Definition of Done:** exact four-path diff; S checks and `git diff --check`; bounded checker/source provenance; complete procedure schema and evidence states; then four separate, fresh independent cold `APPROVE` verdicts and lead scope verification before integration. Checker PASS is structural only.
- **Invariants:** Cargo package name is authoritative; target declaration is not execution; fuzz placement is not implementation ownership; source reachability is not runtime observation; inputs and unsafe boundaries are package-specific; no credentials/provider/runtime state are used; any byte change invalidates that artifact's prior review.

For a handoff, record baselines, exact file scope, checker outcomes, source drift, unresolved shared relation and peer, and execution state per PROC. A successful documentation checker is structural only.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership skill](../../../../.claude/skills/own-corelink-tenant-path-fuzz/SKILL.md#s01)
