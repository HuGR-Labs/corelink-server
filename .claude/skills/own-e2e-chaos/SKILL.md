---
name: own-e2e-chaos
description: Use for source-grounded ownership updates to Cargo package e2e-chaos at tests/e2e-chaos; trigger on owner routing, static contracts, or maintenance-doc changes.
metadata:
  evidence-set: e2e-chaos-static-source-20260921
  source-commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
  manifest: tests/e2e-chaos/Cargo.toml
  profile: S
  package: e2e-chaos
  evidence: static-source-only
---

# Own e2e-chaos documentation

[S01](#s01) · [S02](#s02) · [S03](#s03) · [S04](#s04) · [S05](#s05) · [S06](#s06) · [S07](#s07)

<a id="s01"></a>
## S01 — Identity and scope

Use this skill for ownership work on the Rust package identified by `[package].name = "e2e-chaos"` in `tests/e2e-chaos/Cargo.toml`. The package library is `tests/e2e-chaos/src/lib.rs`; its manifest declares twelve integration targets, a normal `corelink-chaos-scheduler` dependency, and dev-only `proptest`. The description is intent, not execution evidence.

The package author's surfaces are exactly `.claude/skills/own-e2e-chaos/SKILL.md` and `docs/ownership/crates/e2e-chaos/{REFERENCE,BLAST_RADIUS,MAINTENANCE}.md`. Source, manifests, tests, shared indexes, and other packages are outside this authoring scope.

<a id="s02"></a>
## S02 — Invoke, route, or refuse

Invoke for concrete e2e-chaos ownership work: (1) a `make_experiment` mapping changed and its contract needs updating; (2) a new assertion in one declared target needs a source-only inventory; or (3) a dependency, callback, lock, or target change needs impact and maintenance routing.

Do not invoke for concrete out-of-scope cases: (1) changing `corelink-chaos-scheduler` implementation belongs to that package owner; (2) establishing whether CI or a deployed artifact reached this code requires the runtime/build owner and evidence. Use linked `built-not-wired` only to classify the reachability question; this static task does not run Cargo or inspect a deployment.

Delegate imported scheduler API ownership questions to `corelink-chaos-scheduler`; keep this package's consumed signature and call site documented locally. Do not infer runtime selection from workspace membership or target names.

Refuse a request to run a production/provider chaos drill. This documentation authority permits static evidence only.

Refuse a request to edit Rust source, manifests, tests, shared ownership indexes, or another package's artifacts under this package task; route the change to the owning package lead.

<a id="s03"></a>
## S03 — Direct document and skill routes

For symbol, constructor, helper, or test-source contracts, open the [package reference, R02–R06](../../../docs/ownership/crates/e2e-chaos/REFERENCE.md#r02).

For a dependency, imported API, local callback, or consumer arrow, open the [blast relation index, B03](../../../docs/ownership/crates/e2e-chaos/BLAST_RADIUS.md#b03).

For safe update, checking, recovery, or escalation steps, open the [maintenance guide, M01](../../../docs/ownership/crates/e2e-chaos/MAINTENANCE.md#m01).

Use [okf-context](../okf-context/SKILL.md) to route canonical architecture context. Use [built-not-wired](../built-not-wired/SKILL.md) for questions about code presence versus artifact reachability; do not run its Cargo or artifact procedures under this static-only task.

Use the repository's [techlead skill](../techlead/SKILL.md) and [Code Review Checklist](../../../docs/internal/CODE-REVIEW-CHECKLIST.md) for review routing. No separate review skill is defined here.

<a id="s04"></a>
## S04 — Invariant and stop rules

**Condition:** a manifest, source branch, public signature, test assertion, or relation changes. **Action:** update the matching package record and state a falsifier. **Evidence:** cite the repository-relative file and lines. **Stop:** if a claim depends on dependency resolution, CI selection, runtime input, external service, or production wiring, record it as unknown and route it to that owner.

**Condition:** a test comment describes a safety result. **Action:** record only assertions present in test source. **Evidence:** cite the test body and fake implementation. **Stop:** do not claim the scenario passed, capture was skipped, a rollback ran, or an external effect occurred without evidence that observes it.

**Condition:** the canonical policy route is needed. **Action:** follow the designated OKF route. **Evidence:** link the verified profile through R08. **Stop:** do not copy or redefine OKF policy in these package documents.

<a id="s05"></a>
## S05 — Ordered change workflow

1. Pin the requested baseline and confirm the worktree; identify the package from its manifest name. 2. Load only the relevant OKF context and canonical package sections using S03. 3. Classify the requested claim as manifest declaration, source implementation, test-source assertion, relation, or unknown. 4. Enumerate affected symbols, test sources, and direct REL records; route upstream contract changes to their owner. 5. Edit only the authorized ownership artifact or artifacts;

preserve the local-fake and production boundary. 6. Run the documentary checks selected by S06 and correct failed structure or links. 7. Record the evidence, residual unknowns, and required independent review in the S07 handoff.

<a id="s06"></a>
## S06 — Verification selection

For an ownership-document edit, use the supplied checker via `$CHECKER <artifact> --kind <kind> --profile S --root .`, then run `git diff --check <baseline> HEAD`. Check every changed artifact. A structural pass checks format and navigation only; it is not semantic approval or cold review.

Never run Cargo, builds, Rust tests, property tests, fuzzers, provider calls, runtime drills, network requests, GitHub operations, deployment, or production actions for this package-document task. If the checker is unavailable, record the gate as blocked rather than substituting another procedure.

<a id="s07"></a>
## S07 — Handoff

Report the objective and baseline; exact affected artifact paths; affected contracts and REL IDs; exact gates with actual results; residual risks and unresolved unknowns; and the next owner for each routed question. Include the commit SHA and confirm the changed-path scope.

Request a fresh independent review of every changed artifact using the linked repository review guidance. Keep the document state as draft until that review is complete; the author does not self-approve.

The [verified OKF profile](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is the sole designated canonical routing reference.
