---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-chaos
manifest: tests/e2e-chaos/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-chaos-static-source-20260921
---

# e2e-chaos — maintenance guide

Bounded documentary procedures for the source-only package inventory. No procedure authorizes Cargo, test execution, provider access, network, GitHub, deployment, or production activity.

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) · [Evidence matrix](#m04) · [Recovery](#m05) · [Escalation](#m06)

<a id="m01"></a>
## M01 — Safe preparation

**Mode:** `READ_ONLY`. Use the assigned source baseline and a named checkout. Confirm the package is `e2e-chaos` from `tests/e2e-chaos/Cargo.toml:1-20`; review only its manifest, source, tests, and ownership artifacts. Do not load `.env`, keys, provider credentials, customer data, or production configuration. Keep changes to the four owned documents. Route canonical policy only through the [verified OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md). The package description is intent text, not evidence that a scenario executed.

<a id="m02"></a>
## M02 — Procedure selection

| Signal | Procedure | Mode |
|---|---|---|
| package or target changed | [PROC-001](#proc-001) | `READ_ONLY` |
| source contract changed | [PROC-002](#proc-002) | `READ_ONLY` |
| relationship boundary changed | [PROC-003](#proc-003) | `READ_ONLY` |
| documentary artifact needs checking | [PROC-004](#proc-004) | `READ_ONLY` |

Stop when facts require a runtime result, CI selection, scheduler invocation, or external-system evidence. Record the unknown and identify its evidence owner; do not attempt to obtain it through execution in this task.

<a id="m03"></a>
## M03 — Bounded procedures

<a id="proc-001"></a>
### PROC-001 — Reconcile package identity and target inventory
**Mode:** `READ_ONLY`. **Trigger:** manifest or target declaration changes.
1. Read `[package]`, `[lib]`, dependencies, and every `[[test]]` entry.
2. Compare each declared test path with source inventory; keep unit tests distinct.
3. Update R01/R06 and cite the manifest/source lines.
**Expected:** names and counts match visible declarations. **Stop:** selection or CI behavior is requested. **Recovery:** restore the prior documentary claim from baseline. **Evidence:** baseline, paths, lines, resulting target list. [Selection index](#m02)

<a id="proc-002"></a>

### PROC-002 — Reconcile harness source contracts
**Mode:** `READ_ONLY`. **Trigger:** public surface, mapping, helper, fake, or local lock changes.
1. Read changed source and its direct package test call sites.
2. Record the exact branch/signature and a falsifiable predicate.
3. Separate imported scheduler behavior from local harness behavior.
4. Update REFERENCE and any affected relation or maintenance row.
**Expected:** every claim cites a source anchor. **Stop:** semantics depend on an uninspected upstream or runtime contract. **Recovery:** retain the uncertainty and revert unsupported prose. **Evidence:** changed path/lines, source owner, falsifier. [Selection index](#m02)

<a id="proc-003"></a>

### PROC-003 — Reconcile a relationship boundary
**Mode:** `READ_ONLY`. **Trigger:** dependency, import, callback, test consumer, or ownership boundary changes.
1. Trace one producer to one consumer through one named surface.
2. Record activation condition and the nearest failure boundary.
3. Keep Cargo declaration distinct from selected builds and source call site distinct from observed invocation.
4. Route upstream API ownership questions to that package's owner.
**Expected:** each relation is one arrow with one failure boundary. **Stop:** evidence does not identify a consumer or activation condition. **Recovery:** remove the unsupported arrow and list the unknown. **Evidence:** manifest/source anchors and owner. [Selection index](#m02)

<a id="proc-004"></a>

### PROC-004 — Run documentary structure checks
**Mode:** `READ_ONLY`. **Trigger:** any artifact edit.
1. Resolve `$CHECKER` to the supplied checker; do not embed an ephemeral path in docs.
2. Check each owned artifact at profile S with the matching kind and repository root.
3. Run `git diff --check` against the assigned baseline and confirm the exact four-path scope.
4. Record checker output and any blocked check; do not convert a structural pass to approval.
**Expected:** structural and whitespace results are reproducible. **Stop:** checker unavailable or errors remain. **Recovery:** fix only owned prose/anchors or report blocked. **Evidence:** checker path via environment, invocation, result, baseline. [Selection index](#m02)


<a id="m04"></a>
## M04 — Evidence limits matrix

| Documentary evidence | Supports | Does not support |
|---|---|---|
| manifest declarations | identity, dependency declarations, target names | resolved graph, selection, execution |
| source branches | local mapping and fake contracts | scheduler runtime or provider behavior |
| test assertions | authored predicates | passing result or observed safety |
| structural checker | format, navigation, profile bounds | semantic completeness or approval |

<a id="m05"></a>
## M05 — Recovery and compatibility

If a claim cannot be tied to a visible source anchor, remove or mark it unknown before handoff. Restore only the affected owned document text from the named baseline; preserve unrelated worktree changes. A harness-document correction does not change a test, scheduler, runtime state, lock, metric, audit sink, or deployment. For an upstream type or API change, update the ownership boundary and coordinate with that source owner; do not duplicate its policy or edit its package documents from this task.

<a id="m06"></a>
## M06 — Escalation and evidence record

Escalate imported scheduler contract questions to `corelink-chaos-scheduler`. Escalate canonical routing questions through the designated OKF profile. CI selection, resolved features, provider reachability, runtime safety, persistence, rollback, telemetry delivery, and production wiring require evidence from their respective owners and remain unknown here. A handoff records baseline, changed owned paths, source anchors, procedure, evidence mode, checker result, whitespace result, and unresolved questions. A cold review is separate; the author cannot self-approve.

When the v1.3 registry gate is integrated, its procedure population must match exactly `PROC-001` through `PROC-004`; each registry row carries the canonical ID/mode, `execution_status`, `required_for_acceptance`, environment, result, evidence references, and limitations. This manual does not certify registry synchronization or procedure execution.
