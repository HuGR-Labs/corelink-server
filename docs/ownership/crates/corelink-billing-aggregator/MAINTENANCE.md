---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-billing-aggregator
manifest: crates/corelink-billing-aggregator/Cargo.toml
source_commit: 9f372cc1f5a34752a6b6eb4674df10b3921bce88
profile: S
state: author_validated
evidence_set: w003-aggregator-source-static-20260920
---

# corelink-billing-aggregator — maintenance guide

These procedures are bounded to source inspection and ownership-document validation. They do not authorize Cargo compilation, tests, production operations, provider calls, data writes, deployment, or publication. Every result must name its evidence class; this artifact currently supports SOURCE and static document-check evidence only.

[Mode](#m01) · [Intake](#m02) · [Contract change](#m03) · [Failure analysis](#m04) · [Validation](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Mode, prerequisites, and safeguards

| Item | Requirement |
|---|---|
| Mode | Read source and validate documentation only |
| Baseline | Confirm the fixed commit and package manifest before interpreting evidence |
| Inputs | Requested change, exact source paths, and companion ownership documents |
| Expected predicate | Every claim can be classified as SOURCE, static check result, or explicit unknown |
| Stop | Provider, runtime, reverse-consumer, or durable-storage evidence is required |
| Recovery | Preserve the baseline and report the unmet evidence requirement |

Do not treat manifest prose, deferred-wiring comments, a fake, or a successful document checker as operational evidence. The package’s owner artifacts also do not make the author an independent reviewer.

<a id="m02"></a>
## M02 — Intake and scope procedure

[PROC-001](#proc-001).

<a id="proc-001"></a>
### PROC-001 — Classify an aggregator change

**Mode:** SOURCE inspection. **Prerequisites:** fixed baseline, manifest, relevant source module, and requested target. **Expected predicate:** the change is assigned to aggregate/API, order/window, chain, store, audit, or error boundary.

1. Confirm package identity in `Cargo.toml` and current baseline.
2. Read [R03–R07](REFERENCE.md#r03) for the affected public surface and invariant.
3. Read the matching relation in [B02–B05](BLAST_RADIUS.md#b02).
4. Record dependency, source-flow, and impact directions separately.
5. Mark every uninspected consumer or runtime component as unknown.
6. Stop if fulfillment depends on provider behavior or durable implementation.

**Evidence:** source path/line or static manifest field. **Recovery:** restore the classification to “unknown” and request the missing evidence class. [Index](#m02)


<a id="m03"></a>
## M03 — Contract and invariant procedure

[PROC-002](#proc-002).

<a id="proc-002"></a>
### PROC-002 — Review a source contract change

**Mode:** SOURCE inspection. **Prerequisites:** scoped diff and a resolved target boundary. **Expected predicate:** public shape and state-transition impact are explicit before code changes are accepted.

1. Identify affected API records and public reexports. 2. For aggregate shape or serialization, trace canonical-byte and chain-formula impact. 3. For ordering/window changes, trace selection, duplicate data comparison, and aggregate payload impact. 4. For store changes, trace group key, matching-digest overwrite with no head advance, inserted-only head advance, and digest-mismatch behavior. 5. For audit changes, trace start/completion/failure ordering and error propagation. 6. Require compatibility review when reverse consumers are unresolved.

7. Stop if the requested behavior asserts durability, atomicity, or reachability absent from source.

**Evidence:** source-defined signatures, branches, and invariant table. **Recovery:** retain existing contract and return the proposed change for cross-package or operational design. [Index](#m03)


<a id="m04"></a>
## M04 — Failure-path analysis procedure

[PROC-003](#proc-003).

<a id="proc-003"></a>
### PROC-003 — Analyze a reported aggregation failure

**Mode:** source-level reasoning only. **Prerequisites:** reported symptom and reproducible input description, if available. **Expected predicate:** the symptom maps to a documented error/decision path without inventing runtime facts.

1. Separate observed facts from a hypothesized source path. 2. Map invalid windows to `Internal`, canonicalization to `Canonicalization`, and head/sequence disagreement to `ChainBreak` only when inputs support it. 3. Map sink and store result paths using API-003–005 and REL-012/016. 4. Check whether an empty or duplicate decision is consistent with filtering and typed-data equality. 5. State that store failure plus a failing failure-audit may return audit error because of propagation

order. 6. Stop before claiming a provider rollback, alert, retry, or durable audit record.

**Evidence:** SOURCE decision path; an incident report alone is not OBSERVED_RUNTIME proof. **Recovery:** preserve the report, list required logs/traces, and escalate to the operational owner. [Index](#m04)


<a id="m05"></a>
## M05 — Documentation validation procedure

[PROC-004](#proc-004).

<a id="proc-004"></a>
### PROC-004 — Validate the ownership artifacts

**Mode:** static local documentation validation. **Prerequisites:** exactly the four scoped ownership artifacts, Python checker supplied by the integration workflow, and a clean target scope. **Expected predicate:** each artifact passes its declared S-profile structural check and the diff has no whitespace error.

1. Run the candidate checker once for the skill with kind `skill`, profile `S`, and repository root.
2. Run it once each for reference, blast radius, and maintenance with matching kinds/profile/root.
3. Run `git diff --check` on the scoped worktree.
4. Read failures as structural findings, not semantic approval.
5. Correct only authorized artifact paths, then re-run the failed check.
6. Stop if a correction requires shared indexes, source, status, or runtime actions outside scope.

**Evidence:** checker JSON and command exit statuses. **Recovery:** retain failing output and hand off the exact structural error; do not represent a pass as cold review. [Index](#m05)


<a id="m06"></a>
## M06 — Handoff, quality, and definition of done

The handoff must include baseline SHA, changed paths, affected API/invariant/relation/procedure IDs, checker commands and exit statuses, and explicit unknowns. Success means the requested ownership documentation has a bounded source evidence basis and preserves the production boundary. Completeness means M01–M05 provide mode, prerequisites, predicate, stop/recovery, and evidence for normal maintenance decisions. Quality means source claims are falsifiable and operational claims remain unmade.

Definition of done: all four artifacts exist in their assigned paths; each passes the candidate structural checker; `git diff --check` passes; and a scope-only diff shows no unauthorized path. These checks do not execute Rust, prove feature resolution, establish reverse-consumer compatibility, or certify Cron/D1/R2/Queue/audit/chain runtime operation. Independent cold review remains required.

[Back to start](#m01)
