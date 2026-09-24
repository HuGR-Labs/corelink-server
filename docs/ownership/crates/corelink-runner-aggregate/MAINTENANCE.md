---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-runner-aggregate
manifest: crates/corelink-runner-aggregate/Cargo.toml
source_commit: cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6
profile: S
state: author_validated
evidence_set: runner-aggregate-main-readback-20260923
---

# corelink-runner-aggregate — maintenance guide

Every procedure is SOURCE or static-documentary only. It does not authorize Cargo/build/test execution, runner operation, binary invocation, workflow action, D1/Stripe/provider contact, credential use, deployment, or data mutation. Verified OKF is routed through the [wave plan](../../WAVE_010_PLAN.md), not reproduced or revalidated.

[Mode](#m01) · [Intake](#m02) · [Aggregate contract](#m03) · [Failure analysis](#m04) · [Validation](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Mode and baseline

**PROC-001 fields:** objective/trigger = begin package change/review; mode = `READ_ONLY`; environment/permissions = pinned checkout, no network; inputs/preconditions = requested SHA, manifest, library, bin, and integration-test source; expected predicate = identity/scope match pinned record; stop = baseline or scope differs; recovery = obtain reconciled snapshot; evidence = revision/path output. Review `BLOCKED`, execution `REVIEWED_NOT_EXECUTED`, result `NOT_EXECUTED`, acceptance required, execution evidence none; limitation = no Cargo/runtime fact.

| Item | Requirement |
|---|---|
| Mode | `READ_ONLY` inspection and static documentation validation only |
| Baseline | Confirm `cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6`, manifest, library, bin, and integration-test paths |
| Predicate | Every statement is SOURCE, documentary-check evidence, or an explicit unknown |
| Stop | A runner, binary, workflow, D1, billing, provider, or deployment fact is required |
| Recovery | Preserve the source classification and hand off the missing evidence need |

Do not treat comments, static tests, target declaration, imports, or a documentation-check pass as runtime or cold-review evidence.

<a id="m02"></a>
## M02 — Intake and scope procedure

**PROC-002 fields:** objective/trigger = scoped source or manifest change; mode = `READ_ONLY`; environment/permissions = pinned local source, no credentials or network; inputs/preconditions = SHA, manifest, target, changed path; expected predicate = change classified among the listed local contract families; stop = runtime/store/provider assertion required; recovery = route to defining or operational owner; evidence = source signature/branch. Review `BLOCKED`, execution `REVIEWED_NOT_EXECUTED`, result `NOT_EXECUTED`, acceptance required, execution evidence none; limitation = no reverse graph/runtime evidence.

**Mode:** `READ_ONLY`. **Prerequisites:** fixed baseline, requested target, manifest, and affected local source path. **Predicate:** the change is classified as input/output/error shape, grouping/ordering, chain-output mapping, shadow ledger, or static binary target.

1. Confirm R01–R03 and the exact source target.
2. Read the matching API/axiom in R04–R08 and relation in B03–B05.
3. Record local code, imported contract, and uninspected consumer separately.
4. Mark every operational or reverse-consumer need unknown.
5. Stop if the change needs a durable/store/workflow/billing assertion.

**Evidence:** manifest field or source signature/branch. **Recovery:** return the request to the defining package or operational owner with the missing boundary stated.

<a id="m03"></a>
## M03 — Aggregate and compatibility procedure

**PROC-003 fields:** objective/trigger = aggregate, chain, pricing-shape, or public-field change; mode = `READ_ONLY`; environment/permissions = pinned local source only; inputs/preconditions = scoped diff and source predicate; expected predicate = affected INV/API/REL records match current branches; stop = durable idempotency, charge, runner or provider assertion; recovery = preserve contract and request owner evidence; evidence = source fields/call arguments. Review `BLOCKED`, execution `REVIEWED_NOT_EXECUTED`, result `NOT_EXECUTED`, acceptance required, execution evidence none; limitation = imported/runtime semantics not re-established.

**Mode:** `READ_ONLY`. **Prerequisites:** scoped diff and a stated source predicate. **Predicate:** all changed local state/output relations remain explicit and falsifiable.

1. For version, terms, and prior-consumption validation, trace API-001/002, INV-001/002, and REL-010.
2. For staged records, decoded-key deduplication, and counters, trace API-003, INV-003/004, and REL-006/011.
3. For aggregate identifiers/prior heads/digests/updates, trace API-004, INV-005, and REL-002/012.
4. For cumulative shadow output, trace API-005, INV-006, and REL-003/013.
5. For public fields/errors/bin target and subprocess contract, trace API-006, R06, and REL-004/005/009/014/015.
6. Require external compatibility review when reverse users are unresolved.
7. Stop before asserting any runner operation, durable idempotency, charge, or provider action.

**Evidence:** source fields, branches, imported-call arguments, and the six invariant predicates. **Recovery:** retain the existing contract and request defining-package or operational evidence where needed.

<a id="m04"></a>
## M04 — Static failure-path analysis

**PROC-004 fields:** objective/trigger = reported malformed input or local branch change; mode = `READ_ONLY`; environment/permissions = pinned source, no binary invocation; inputs/preconditions = supplied input description or diff; expected predicate = report maps to a located error branch; stop = result requires observed execution; recovery = mark unverified and route to evidence owner; evidence = `lib.rs` and bin source. Review `BLOCKED`, execution `REVIEWED_NOT_EXECUTED`, result `NOT_EXECUTED`, acceptance required, execution evidence none; limitation = no runner operation established.

**Mode:** `READ_ONLY` source reasoning. **Prerequisites:** reported input description or scoped source diff. **Predicate:** the report maps to a local error branch without inventing execution facts.

1. Map invalid idempotency-key text to `MalformedEvent` only when the input fails the fixed-size hex parse.
2. Map invalid supplied head text to `MalformedChainHead` only when that region is visited.
3. Map imported append rejection to local `ChainBreak` without restating the imported algorithm.
4. For bin-source changes, distinguish local parse/read/serialize/write branches from actual invocation.
5. List logs, store state, scheduler records, or billing evidence needed for any operational conclusion.

**Evidence:** `src/lib.rs` and declared bin source. **Recovery:** preserve the report as unverified and route it to the owner holding observed/runtime evidence.

<a id="m05"></a>
## M05 — Documentary validation procedure

**PROC-005 fields:** objective/trigger = artifact bytes change; mode = `READ_ONLY`; environment/permissions = local checkout and supplied checker; inputs/preconditions = assigned four files/profile S; expected predicate = four matching structural checks and whitespace diff pass; stop = nonzero or scope drift; recovery = fix scoped finding and rerun; evidence = JSON outputs, `git diff --check`, path list. Review `BLOCKED`, execution `REVIEWED_NOT_EXECUTED`, result `NOT_EXECUTED`, acceptance required, execution evidence none; limitation = checker is not semantic approval.

**Mode:** `READ_ONLY` static local documentation validation. **Prerequisites:** exactly four assigned ownership artifacts, the supplied checker, and a scoped checkout. **Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; each matching S-profile checker passes; the diff is whitespace-clean and path-scoped.

1. Run the supplied checker once for the skill with kind `skill`, profile `S`, and repository root.
2. Run it once each for reference, blast radius, and maintenance with their matching kinds/profile/root.
3. Run `git diff --check` and inspect changed paths against the four assigned artifacts.
4. Treat a pass as structure only, not source semantic approval, Cargo/test result, runner operation, or cold review.
5. Correct only an assigned artifact; stop on a source/scope/operational requirement.

**Evidence:** four checker JSON verdicts, `git diff --check` exit status, and changed-path list. **Recovery:** retain the exact failing finding and amend only the authorized path or hand it off.

<a id="m06"></a>
## M06 — Handoff, quality, and definition of done

**PROC-006 fields:** objective/trigger = conclude a bounded static assessment; mode = `READ_ONLY`; environment/permissions = local checkout; inputs/preconditions = four scoped artifacts and source evidence; expected predicate = IDs, checks, unknowns and changed paths recorded; stop = any unresolved runtime/consumer fact is presented as proven; recovery = retain unknown and route it; evidence = SHA, source paths, checker output, diff. Review `BLOCKED`, execution `REVIEWED_NOT_EXECUTED`, result `NOT_EXECUTED`, acceptance required; execution evidence none; limitation = independent cold review pending.

Handoff includes baseline SHA, manifest/source paths read, changed paths, affected API/invariant/relation IDs, checker commands and exit statuses, skipped checks, and explicit unknowns. Success means bounded static ownership documentation with no runner-operation assertion. Completeness means M01–M05 provide mode, prerequisite, predicate, evidence, stop, and recovery; the four artifacts contain S01–S07, R01–R08, B01–B06, and M01–M06. Quality means claims remain falsifiable and imported/runtime behavior remains bounded.

Definition of done: four assigned files exist; four S-profile checker passes and `git diff --check` pass are recorded; scope review contains no other path; independent cold review remains pending. These checks do not build or test Rust, operate a runner, invoke a binary, prove a workflow/D1/Stripe path, establish billing, or certify deployment.

[Back to start](#m01)
