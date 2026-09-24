---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-failover-router
manifest: tests/e2e-failover-router/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: draft
evidence_set: e2e-failover-router-static-20260921
---

# e2e-failover-router documentary maintenance

[M01](#m01) · [M02](#m02) · [M03](#m03) · [M04](#m04) · [M05](#m05) · [M06](#m06)

<a id="m01"></a>
## M01 — Scope and evidence mode

This record governs the package task's four assigned artifacts: `.claude/skills/own-e2e-failover-router/SKILL.md`, `REFERENCE.md`, `BLAST_RADIUS.md`, and `MAINTENANCE.md` under `docs/ownership/crates/e2e-failover-router/`. A repository-wide campaign diff can contain other packages and shared evidence; this manual's scope check is path-scoped to these four files. Mode: `READ_ONLY` for source and `LOCAL_ISOLATED` for document checks. The package boundary and limits are in [R01–R08](REFERENCE.md#r01) and [B01–B06](BLAST_RADIUS.md#b01).

<a id="m02"></a>
## M02 — Refresh source facts

**Mode:** `READ_ONLY`. **Prerequisite:** identify the changed package source or manifest path and pin the inspected revision. **Predicate:** every changed fact has a path and line anchor; manifest declaration, direct source use, upstream owner, and unknown remain distinct. **Action:** inspect only the needed package files and upstream contract source; update the relevant R and B records and matching skill route.

**Evidence:** cited source paths and pinned revision. **Stop:** if a claim needs execution, resolved Cargo selection, complete reverse callers, operational policy, or OKF adjudication, record it unknown and route it. **Recovery:** remove unsupported wording and restore the static boundary.

<a id="m03"></a>
## M03 — Update one local contract or assertion

**Mode:** `READ_ONLY`. **Prerequisite:** source owner has made or authorized a local helper/assertion change. **Predicate:** only the observed clock, ledger, fixture, or test assertion changes in this record; an upstream contract remains attributed to its crate. **Action:** trace the changed symbol and its package-local references; update [R04–R05](REFERENCE.md#r04) and the single affected arrow in [B02–B06](BLAST_RADIUS.md#b02).

**Evidence:** changed source anchor and assertion text. **Stop:** if the requested conclusion requires a run result or live failover claim. **Recovery:** remove claims whose source predicate cannot be located.

**Procedure index:** [PROC-001 — refresh the target population](#proc-001) · [PROC-002 — trace an assertion change](#proc-002) · [PROC-003 — document failure and observability](#proc-003) · [PROC-004 — validate this artifact set](#proc-004).

<a id="proc-001"></a>
### PROC-001 — Refresh the target population
**Objective/trigger:** a manifest, target, feature, CI selector, or scenario file changes. **Preconditions/inputs:** pinned source revision; `Cargo.toml`, package `src/`, `tests/`, and the workflow files found by read-only search. **Mode/environment/permissions:** `READ_ONLY`, local checkout.

1. Record the package name, manifest targets, declared features, and dependency categories from the manifest.
2. Enumerate tracked package source, integration-test, example, bench, and fixture files; keep declared targets separate from any CI selector found.
3. Search workflow/config files for the manifest path, package name, test target, and scenario names; record exact matches and any unsearched population.
4. Update R01/R03/R06 and B02/B03/B06 with paths, revision, and explicit unknowns.

**Expected predicate:** inventory claims name their scanned population and revision; no workflow or test execution is inferred. **Stop:** if a build graph, executed test, staging or production drill is needed. **Recovery:** narrow claims to source declarations and mark selection unknown. **Evidence:** manifest/source/workflow paths, query terms, revision, and result. **State:** `REVIEWED_NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-002"></a>
### PROC-002 — Trace a scenario or assertion change
**Objective/trigger:** a helper, scenario, route assertion, lease assertion, graph check, or audit assertion changes. **Preconditions/inputs:** exact diff and R04–R05 contracts. **Mode/environment/permissions:** `READ_ONLY`, local source.

1. Follow the changed symbol from fixture construction through the assertion; link the exact source lines.
2. Identify whether the state is package-local, supplied by `corelink-failover-router`, or re-exported from `corelink-replica-worker`.
3. Record the failure witness and limitation in R04/R05, then update only the affected atomic relation in B03/B04/B05.
4. If assertion execution is relevant, state the exact command as a validation requirement; do not claim it ran.

**Expected predicate:** the changed assertion has a falsifier, owner, relation ID, and bounded interpretation. **Stop:** if it requires route runtime, real lease enforcement, audit transport, or an operational drill. **Recovery:** state `UNKNOWN` and route to the owning package/authorized operator. **Evidence:** changed source lines, relation ID, source revision, and proposed validation command. **State:** `REVIEWED_NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-003"></a>
### PROC-003 — Document an induced failure and observability boundary
**Objective/trigger:** router error, failed probe/sink, mutex poison, or telemetry claim changes. **Preconditions/inputs:** source error variants, call order, fake implementation, and any cited observability interface. **Mode/environment/permissions:** `READ_ONLY`.

1. Locate the typed error/result branch and the caller assertion or propagation point.
2. Separate scenario-local records from router-produced records and identify whether a metric/log/alert is declared in this package or only upstream.
3. Add the failure path, propagation boundary, validation source, and evidence class to R07 and its relation; preserve “not executed” where no run exists.

**Expected predicate:** every failure claim has a source branch and an observability owner or explicit unknown. **Stop:** when proving delivery, alerting, or live recovery requires execution or external access. **Recovery:** remove delivery claims and escalate with source anchors. **Evidence:** error definitions, caller branch, and record/metric source. **State:** `REVIEWED_NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-004"></a>
### PROC-004 — Validate the four ownership artifacts
**Objective/trigger:** any assigned artifact byte changes. **Preconditions/inputs:** current root, four package paths, checker, and scoped diff. **Mode/environment/permissions:** `LOCAL_ISOLATED`; local document tooling only.

1. Run the S-profile checker once for skill, reference, blast-radius, and maintenance.
2. Run `git diff --check` scoped to the four paths and inspect `git status --short` for those paths.
3. Save exact outputs, revision, and hashes; request a fresh independent cold review for changed bytes.

**Expected predicate:** all four structural checks pass and scoped diff is clean. **Stop:** on a failed check, unowned path, missing cold review, or a request for Cargo/runtime proof. **Recovery:** fix only the assigned documents, repeat checks, and keep semantic/runtime status unknown. **Evidence:** command lines, outputs, hashes, and reviewer verdicts. **State:** structural validation is executed locally only when outputs are actually captured; review remains separate.
[Procedure index](#m03)

<a id="m04"></a>
## M04 — Structural documentary checks

**Mode:** `LOCAL_ISOLATED`. **Prerequisite:** the four package artifact paths are identified, even if other campaign paths are also changed. **Predicate:** supplied profile-S checks pass once per artifact and whitespace checks pass for these paths. **Action:** use `docs/ownership/tools/check_docs.py`, run these commands, and inspect only the package-scoped path set; integration owns repository-wide scope reconciliation:

```sh
python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-e2e-failover-router/SKILL.md
python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/e2e-failover-router/REFERENCE.md
python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/e2e-failover-router/BLAST_RADIUS.md
python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/e2e-failover-router/MAINTENANCE.md
git diff --check -- .claude/skills/own-e2e-failover-router/SKILL.md docs/ownership/crates/e2e-failover-router/REFERENCE.md docs/ownership/crates/e2e-failover-router/BLAST_RADIUS.md docs/ownership/crates/e2e-failover-router/MAINTENANCE.md
git diff --name-only HEAD -- .claude/skills/own-e2e-failover-router/SKILL.md docs/ownership/crates/e2e-failover-router/REFERENCE.md docs/ownership/crates/e2e-failover-router/BLAST_RADIUS.md docs/ownership/crates/e2e-failover-router/MAINTENANCE.md
```

**Evidence:** checker JSON and diff status, explicitly structural only. **Stop:** on nonzero result, scope drift, unresolved source fact, or missing checker; repair only an owned artifact or hand off the blocker.

**Recovery:** rerun the four checks after any byte change. No check result is an approval.

<a id="m05"></a>
## M05 — Independent review and handoff

**Mode:** `READ_ONLY`. **Prerequisite:** artifact bytes are final and structural outputs are recorded. **Predicate:** a cold reviewer separately evaluates each artifact against the wave criteria. **Action:** hand off baseline, four paths, direct source arrows, check outputs, and unknowns.

**Evidence:** independent review record from the reviewer. **Stop:** if the reviewer or execution evidence is absent; label review or runtime `UNKNOWN`. **Recovery:** any byte change returns to M04 and requires a fresh review. Authors must not self-approve.

<a id="m06"></a>
## M06 — Boundaries and escalation

Escalate router or probe semantics to `corelink-failover-router`; region graph, audit taxonomy, and replica semantics to `corelink-replica-worker`; drill execution to the authorized operations owner. The verified [OKF profile](../../../internal/okf-wiki/01-okf-corelink-profile.contract.md) remains a routing destination only. Do not copy or revalidate it. Keep CI selection, resolved graph, test execution, external audit delivery, staging, deployment, and production outcomes explicitly unknown unless their own evidence is supplied.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership skill](../../../../.claude/skills/own-e2e-failover-router/SKILL.md#s01)
