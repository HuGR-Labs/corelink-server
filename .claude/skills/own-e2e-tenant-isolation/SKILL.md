---
name: own-e2e-tenant-isolation
description: >-
  Use for static ownership of the e2e-tenant-isolation harness, its fixtures,
  adversarial assertions, manifest, and declared target. Do not use to infer
  production isolation, provider behavior, or authorize operations.
metadata:
  schema: "corelink-ownership/1.1"
  package: "e2e-tenant-isolation"
  manifest: "tests/e2e-tenant-isolation/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "e2e-tenant-isolation-static-1177dad2c"
  profile: "S"
  contract: "CO-1 1.1-candidate; v1.3 input"
---

# Ownership — e2e-tenant-isolation

[Scope](#s01) · [Authority](#s02) · [Read](#s03) · [Decisions](#s04) ·
[Workflow](#s05) · [Stop](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Scope

| Use this skill when | Use another owner or route when |
|---|---|
| Changing this package's fixed tenant fixtures, in-memory fakes, exported harness types, or 25 adversarial scenario assertions | Changing HMAC prefix implementation or its canonical contract: inspect tenant-path source and its owner |
| Changing this manifest, its library, or the explicit adversarial test target | Changing the audit envelope/emitter or BYOK implementation: coordinate with corelink-audit or corelink-byok ownership |
| Reviewing what this harness does and does not establish | Requesting provider, deployment, production, database, storage, CI, or runtime action |

<a id="s02"></a>
## S02 — Territory and authority

| Area | Ownership boundary |
|---|---|
| Implementation | This package's fixtures, in-memory fake stores, library exports, and adversarial test source |
| Contract | Local harness contracts only; tenant-path prefix, audit, and BYOK APIs remain owned by their source packages |
| Composition | The manifest declares three first-party dependencies. Production handler, D1 outbox, R2/KV, and KMS composition is outside this package and is not established here |
| Runtime and review | No runtime was observed. The author cannot approve these documents; each artifact needs an independent cold review at its final hash |

This is test support, not a production component. This skill grants no operation authority.

<a id="s03"></a>
## S03 — Read only the relevant route

| Question | Read |
|---|---|
| What is declared and implemented? | [Reference](../../../docs/ownership/crates/e2e-tenant-isolation/REFERENCE.md#r01) |
| What can a source change affect? | [Atomic relations](../../../docs/ownership/crates/e2e-tenant-isolation/BLAST_RADIUS.md#b03) |
| Which local documentary procedure applies? | [Maintenance](../../../docs/ownership/crates/e2e-tenant-isolation/MAINTENANCE.md#m02) |
| What canonical isolation context applies? | [OKF tenancy isolation](../../../docs/knowledge/tenancy/isolation.md) and the invariant registry at specs/03_architecture/invariant_registry.md |

Read the source at the pinned commit named in each document. Treat manifest intent, comments, and test names as claims to check against source; none proves execution.

<a id="s04"></a>
## S04 — Decision rules

| Condition | Action | Required evidence | Stop when |
|---|---|---|---|
| A fixture or assertion changes | Trace its producer, consumer, predicate, and scenario ID in R03–R05 and B03–B05 | Pinned file/line, scenario, API/INV/REL IDs, and static-vs-runtime state | The claim needs a provider or production result |
| An API or target changes | Reconcile the exact manifest/source diff and all three first-party edges | Manifest lines, exact signature, target/import census, and changed dependency owner | Target selection or resolved features are required but unavailable |
| Audit-before-reject is claimed | Inspect the individual scenario and AuditCapture call order | `record_deny` lines, scenario lines, and whether the assertion is present | Do not generalize the crate comment: S04 has no audit assertion, and S05 records the deny after BYOK returns AadMismatch |
| A verifier script expects a source marker | Compare both literals to the pinned file and retain any mismatch as unresolved | Script path/hash, source line, command output, and `BROKEN`/`UNKNOWN` state | Do not run or call the check successful in this source-only task |

<a id="s05"></a>
## S05 — Safe workflow

1. Confirm the requested package, pinned source, integration baseline, and four-path scope.
2. Read its manifest, library, fixtures, test target, and each affected external reference.
3. Keep dependency direction, data flow, and impact propagation separate in every REL.
4. Separate harness source behavior from upstream contract, production composition, and runtime evidence.
5. Use only the documentary checker and scoped diff check for this documentation task.
6. Return changed hashes, checker metrics, findings, and unknowns for independent review.

<a id="s06"></a>
## S06 — Required stops

Stop if the pinned package or root workspace manifest differs from the assigned baseline; if target/API facts depend on unperformed Cargo resolution; if a fake is presented as provider behavior; if an external script expectation conflicts with source; or if production, customer data, database, storage, or deployment evidence is requested. The escalation owner for the two residual scripts is not identified in the inspected source; ask the repository lead to identify the responsible owner before changing or relying on them.

<a id="s07"></a>
## S07 — Five acceptance axioms and handoff

| Axiom | Apply it here |
|---|---|
| Success | A maintainer can route harness work, find its contracts, understand the harness/production boundary, and select a safe documentary procedure |
| Completeness | Reconcile exact Cargo identity, targets/features, source modules, falsifiable contracts, every first-party edge, and material non-Cargo references; retain unknowns |
| Quality | Anchor material claims to source; keep each REL atomic with distinct dependency/data/impact directions; stay within S-profile caps |
| Definition of Done | Exactly four scoped artifacts, required sections, manifest declaration records, four structural checks and diff check, then separate cold APPROVE per artifact and lead integration |
| Invariants | Package name is authoritative; declaration is not execution; import is not runtime reachability; fake is not provider behavior; no operational mutation or self-approval |

Return baseline, changed paths and hashes, affected API/INV/REL/PROC IDs, exact static commands and results, unresolved source contradictions, unknowns, and the next owner. Never report a structural check as semantic approval or runtime proof.

[Back to scope](#s01)
