---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-dt-reconcile
manifest: tools/dt-reconcile/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: author_validated
evidence_set: w013-dt-reconcile-static-source-20260921
---

# corelink-dt-reconcile — maintenance guide

Every procedure is SOURCE inspection or static-documentary validation only. It does not authorize Cargo/build/test work, binary invocation, network/API contact, provider access, credential use, queue/data mutation, scheduling, alerting, deployment, or production action. The verified canonical OKF is routed only through the [Wave 013 plan](../../WAVE_013_PLAN.md), without copying, redefining, or revalidating its policy.

[Mode](#m01) · [Intake](#m02) · [Contract](#m03) · [Failure paths](#m04) · [Validation](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Mode and baseline

| Item | Requirement |
|---|---|
| Mode | SOURCE inspection and static-documentary validation only |
| Baseline | Confirm `398e586ccef712477f2a4ce51e026443b67e5747`, manifest, and `tools/dt-reconcile/src/main.rs` |
| Predicate | Every statement is source text, documentary-check output, or explicit unknown |
| Stop | A reconciliation, data, provider, runtime, or operational fact is required |
| Recovery | Keep the evidence boundary and hand off the stated missing evidence need |

Comments, variable names, literals, imports, static branches, and a checker pass are never runtime, provider, delivery, or cold-review evidence.

<a id="m02"></a>
## M02 — Intake and scope procedure

**Mode:** SOURCE. **Prerequisites:** fixed baseline, request, manifest, and affected target source path. **Predicate:** the proposed change is classified as result/exit shape, local count arithmetic, configuration text, imported queue/handler call, retry branch, or binary declaration.

1. Confirm R01–R03 and the scoped target path.
2. Read the matching API/axiom in R04–R07 and relation in B02–B05.
3. Record local source text, imported interface, and uninspected behavior separately.
4. Mark provider, data, delivery, runtime, and reverse-consumer needs unknown.
5. Stop if a conclusion requires an observed outcome.

**Evidence:** target manifest field or source signature/branch. **Recovery:** route the need to the owner holding independently collected evidence.

<a id="m03"></a>
## M03 — Contract and relation procedure

**Mode:** SOURCE. **Prerequisites:** scoped diff and stated predicate. **Predicate:** every changed relation stays atomic, directional, and falsifiable.

1. Trace count/result changes through API-001, INV-001, and REL-003.
2. Trace environment/default/constructor changes through R06 and REL-004.
3. Trace queue/handler/signature calls through API-003, INV-002/004, and REL-005.
4. Trace retry threshold/entry mapping through INV-003 and REL-006.
5. Trace main log/exit selection through API-002, INV-005, and REL-007/008.
6. Stop before calling a source branch a reconciliation, alert, delivery, or data effect.

**Evidence:** `tools/dt-reconcile/src/main.rs` and its manifest. **Recovery:** preserve the local claim and request external-contract or observed evidence as needed.

<a id="m04"></a>
## M04 — Static failure-path procedure

**Mode:** SOURCE reasoning. **Prerequisites:** source diff or reported input description. **Predicate:** the account maps only to a visible local error/branch relationship.

1. Identify propagated source `?` paths without assigning an actual error.
2. Distinguish imported handler `Ok` and `Err` branches from a delivery result.
3. Describe the literal retry guard without claiming a queue write or retry.
4. Distinguish outer `Err` exit selection from a process observation.
5. List provider/data/log/alert/runtime evidence required for any operational conclusion.

**Evidence:** `tools/dt-reconcile/src/main.rs`. **Recovery:** label the report unverified and hand off to an owner with observed evidence.

<a id="m05"></a>
## M05 — Documentary validation procedure

**Mode:** static local documentation validation. **Prerequisites:** exactly four assigned artifacts, supplied checker, and scoped checkout. **Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; each S-profile checker passes; the baseline diff is whitespace-clean and scope-limited.

1. Run the supplied checker for the skill with kind `skill`, profile `S`, and repository root.
2. Run it for reference, blast radius, and maintenance with their matching kinds/profile/root.
3. Run `git diff --check 398e586cc HEAD` and inspect changed paths against the four assigned artifacts.
4. Treat passes as structural only, not semantic approval, execution, runtime, provider, or cold-review evidence.
5. Correct only an assigned artifact; stop on a source, scope, or operational requirement.

**Evidence:** four checker JSON verdicts, diff-check status, and changed-path list. **Recovery:** retain the finding and amend only an authorized file or hand it off.

<a id="m06"></a>
## M06 — Handoff, quality, and definition of done

Handoff includes baseline SHA, manifest/source paths read, changed paths, affected API/invariant/relation IDs, checker commands/statuses, skipped checks, and explicit unknowns. Success means bounded static documentation with no reconciliation/data/provider/runtime assertion. Completeness means M01–M05 specify mode, prerequisites, predicate, evidence, stop, and recovery; all required S/R/B/M sections exist. Quality means every relation is falsifiable and canonical OKF remains route-only.

Definition of done: the four assigned files exist; four S-profile structural checks and baseline diff hygiene pass; the path scope is clean; independent cold review remains pending. These checks do not build/test Rust, invoke the binary, reconcile data, contact a provider, establish a queue/delivery/alert result, or certify deployment.

[Back to start](#m01)
