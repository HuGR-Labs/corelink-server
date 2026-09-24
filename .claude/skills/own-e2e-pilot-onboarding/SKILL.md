---
name: own-e2e-pilot-onboarding
description: >-
  Route source, test-text, and ownership-document changes for the e2e-pilot-onboarding
  harness; do not infer production onboarding, provider effects, or runtime from it.
metadata:
  schema: "corelink-ownership/1.1"
  package: "e2e-pilot-onboarding"
  manifest: "tests/e2e-pilot-onboarding/Cargo.toml"
  source-commit: "cb94e251c0f17382565bf863f517945cbb2a84d6"
  evidence-set: "w014-e2e-pilot-onboarding-source-static-20260921"
---

# Ownership — e2e-pilot-onboarding

[Scope](#s01) · [Authority](#s02) · [Read](#s03) · [Decide](#s04) · [Work](#s05) · [Stop](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Activation and scope

| Activate exactly when | Do not activate for |
|---|---|
| Editing `tests/e2e-pilot-onboarding/Cargo.toml`, its Rust source, one of its five declared target sources, or these four ownership artifacts; e.g. `PilotHarness` behavior, scenario assertion, or package relation. | Production signup, billing, CAS, audit, DSR, or offboarding implementation/operation; canonical policy changes; provider/runtime claims. |

Evidence scope is the fixed source snapshot
`cb94e251c0f17382565bf863f517945cbb2a84d6`. Route production signup implementation
to the verified [signup owner skill](../own-corelink-signup/SKILL.md); route
canonical signup/onboarding policy only to the designated [OKF concept](../../../docs/knowledge/launch/signup-onboarding.md).
Implementation ownership and policy are not interchangeable.

<a id="s02"></a>
## S02 — Territory and authority

The package owns a source-local deterministic harness and its declared local
scenario contracts. `Cargo.toml` declares no direct first-party dependency.
`PilotHarness` uses in-memory state; a manifest description, comments, receipt
text, or test name is not proof of external service composition or execution.
This skill grants no production, provider, customer-data, deployment, or
independent-review authority.

Package implementation owner is `UNASSIGNED` in this record. `.github/CODEOWNERS`
routes a review request to `@gmhelmold`; that is not implementation, operations,
or independent-approval authority. Ask a repository administrator to assign a
package owner; until assignment is recorded, stop work that requires that authority.

<a id="s03"></a>
## S03 — Read by task

| Task | Read directly |
|---|---|
| Identity, public surface, local state, falsifiable invariants | [Reference R01–R08](../../../docs/ownership/crates/e2e-pilot-onboarding/REFERENCE.md#r01) |
| Change a declared target or scenario: `test_01` through `test_05` | Its target relation [REL-013–REL-017](../../../docs/ownership/crates/e2e-pilot-onboarding/BLAST_RADIUS.md#rel-013), plus each method relation called by that target in B03 |
| Export method, export target, or export assertions | [REL-007](../../../docs/ownership/crates/e2e-pilot-onboarding/BLAST_RADIUS.md#rel-007), [REL-039](../../../docs/ownership/crates/e2e-pilot-onboarding/BLAST_RADIUS.md#rel-039), [REL-015](../../../docs/ownership/crates/e2e-pilot-onboarding/BLAST_RADIUS.md#rel-015), and API-009/010 + INV-005/006 in R04–R05 |
| Method/state change and propagated effects | The affected API/INV in [R04–R05](../../../docs/ownership/crates/e2e-pilot-onboarding/REFERENCE.md#r04), then every listed affected relation in [B03–B05](../../../docs/ownership/crates/e2e-pilot-onboarding/BLAST_RADIUS.md#b03) |
| Safe local/documentary action | [Maintenance M01–M06](../../../docs/ownership/crates/e2e-pilot-onboarding/MAINTENANCE.md#m01) |
| Fake-vs-wired question | [built-not-wired skill](../built-not-wired/SKILL.md) |
| OKF context or policy | [okf-context skill](../okf-context/SKILL.md), then designated OKF link in R02 |

Cross-cutting links above are present in this checkout. No local cold-review
skill was verified; obtain a fresh reviewer/context under the campaign review
contract instead of inventing a skill route.

<a id="s04"></a>
## S04 — Decisions and invariants

| Condition | Action → evidence required | Stop when |
|---|---|---|
| Manifest, crate root, or root re-export changes | Trace R01–R03 and REL-001–REL-003; distinguish declared target from resolved selection. | A resolved build/target claim is needed; it is UNKNOWN here. |
| A local method, state field, audit call, or fixture changes | Update its exact API/INV, then every related method, state, and scenario relation in B03–B05; cite source and negative case. | A durable store, external effect, or observed ordering/result is claimed. |
| One test-target declaration or target source changes | Update that target relation and its called-method relations; retain negative assertion cases and label evidence SOURCE. | A test execution or runtime claim is required. |
| Production signup implementation or canonical policy is requested | Production implementation → `own-corelink-signup`; policy → OKF concept. | The request conflates implementation ownership with canonical policy. |
| An ownership artifact changes | Follow [PROC-003 in M03](../../../docs/ownership/crates/e2e-pilot-onboarding/MAINTENANCE.md#proc-003), inspect exact scope, and request fresh independent review. | Any artifact lacks its own fresh review; authorship or CODEOWNERS match is not approval. |

Five evidence axioms govern every conclusion: package identity is `[package].name`;
declarations do not prove resolution; names/fakes/workspace membership do not prove
reachability or runtime; provider execution is out of scope; changed bytes require
fresh independent review.

<a id="s05"></a>
## S05 — Bounded workflow

1. Confirm manifest identity, baseline, and exact changed paths.
2. Read only the affected API/INV, relation fiches, source, and target assertion.
3. Record the falsifiable predicate, failure branch, and all affected direct and transitive relations.
4. Keep source facts, contradictory documentary claims, and unknown runtime facts separate.
5. Use the applicable maintenance procedure; retain actual documentary output and request cold review of final bytes.

<a id="s06"></a>
## S06 — Stop conditions

Stop on a need for Cargo resolution, build/test/fuzz execution, CI results, network,
GitHub, provider, production, customer data, deployment, unverified reverse-consumer
completeness, or an irreversible effect. Record the specific unknown and the owner
or evidence needed. Do not treat a runbook, backlog entry, verifier source, or gapmap
as executed behavior. Do not broaden the four-path ownership edit scope.

<a id="s07"></a>
## S07 — Handoff

Report objective, baseline, exact paths, changed API/INV/REL/PROC IDs, the five
axioms, three unresolved unknown domains in R08, four checker outputs, and
whitespace/scope result.
Call checks DOCUMENTARY, not execution or approval. Request fresh independent
review for every changed artifact; a matching CODEOWNERS rule alone does not prove
that the reviewer is independent or authorized.

[Reference](../../../docs/ownership/crates/e2e-pilot-onboarding/REFERENCE.md#r01) · [Blast radius](../../../docs/ownership/crates/e2e-pilot-onboarding/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/e2e-pilot-onboarding/MAINTENANCE.md#m01)
