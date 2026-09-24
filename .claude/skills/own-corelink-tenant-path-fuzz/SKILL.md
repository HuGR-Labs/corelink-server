---
name: own-corelink-tenant-path-fuzz
description: >-
  Route changes to the corelink-tenant-path-fuzz manifest or its derive_prefix and
  derive_prefix_extended harnesses. Use when reviewing their input layout,
  assertions, dependency boundary, or configured fuzz workflow. Do not use as the
  owner of tenant-path derivation, tenant authorization, storage, or runtime operations.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-tenant-path-fuzz"
  manifest: "crates/tenant-path/fuzz/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  integration-baseline: "8cdc02828132b9b6f03a3b57117b8140325f6762"
  evidence-set: "w016-tenant-path-fuzz-source-20260921"
---

# Ownership — corelink-tenant-path-fuzz

Draft from pinned source. Harness declarations and workflow configuration are not evidence that a fuzz run completed or that production behavior was observed.

[Triggers](#s01) · [Authority](#s02) · [Reading](#s03) · [Decisions](#s04) · [Workflow](#s05) · [Stops](#s06) · [Evidence](#s07)

<a id="s01"></a>
## S01 — Triggers

| Use this skill when | Route elsewhere when |
|---|---|
| Changing either fuzz target, its byte layout, assertions, or direct dependencies | Changing `derive_prefix` implementation or tenant-prefix policy: `corelink-tenant-path` owns that code |
| Reviewing target declarations, independent workspace setup, or fuzz CI selection | Changing authorization, tenant resolution, R2/D1/KV behavior, or deployment: this package does not implement those systems |
| Assessing whether a target covers determinism, encoding, or a claimed separation property | Seeking evidence that fuzzing ran or production behaved a certain way: obtain execution/runtime evidence separately |

<a id="s02"></a>
## S02 — Territory and authority

The package is `corelink-tenant-path-fuzz` at `crates/tenant-path/fuzz/Cargo.toml`. It declares two harness bins which call the parent `corelink-tenant-path` API. The parent package owns the derivation implementation and its contract. This skill covers harness source, declared dependencies, and source-visible workflow wiring only.

No named implementation owner, composition-root operator, runtime operator, or independent reviewer is verified here. This skill grants no authority to run fuzzing, access keys, use providers, modify stored data, deploy, publish, or approve these documents.

<a id="s03"></a>
## S03 — Read by question

| Question | Open |
|---|---|
| What exact input reaches each target? | [R03](../../../docs/ownership/crates/corelink-tenant-path-fuzz/REFERENCE.md#r03) and [R05](../../../docs/ownership/crates/corelink-tenant-path-fuzz/REFERENCE.md#r05) |
| What does the parent API guarantee in source? | [API-001](../../../docs/ownership/crates/corelink-tenant-path-fuzz/REFERENCE.md#api-001) and [API-002](../../../docs/ownership/crates/corelink-tenant-path-fuzz/REFERENCE.md#api-002) |
| Which declared edges and workflows are in scope? | [B03](../../../docs/ownership/crates/corelink-tenant-path-fuzz/BLAST_RADIUS.md#b03) and [B04](../../../docs/ownership/crates/corelink-tenant-path-fuzz/BLAST_RADIUS.md#b04) |
| Which properties are asserted versus only described? | [M04](../../../docs/ownership/crates/corelink-tenant-path-fuzz/MAINTENANCE.md#m04) |
| How should a source-only review proceed? | [PROC-001](../../../docs/ownership/crates/corelink-tenant-path-fuzz/MAINTENANCE.md#proc-001) and [PROC-002](../../../docs/ownership/crates/corelink-tenant-path-fuzz/MAINTENANCE.md#proc-002) |
| What compatibility questions remain open? | [M05](../../../docs/ownership/crates/corelink-tenant-path-fuzz/MAINTENANCE.md#m05) and [B06](../../../docs/ownership/crates/corelink-tenant-path-fuzz/BLAST_RADIUS.md#b06) |
| What canonical context governs the prefix claim? | Load [`okf-context`](../okf-context/SKILL.md) and read [OKF/ADR-0043](../../../docs/knowledge/adr/adr-0043-hmac-tenant-prefix-algorithm.md) before changing the harness; this package records source context, not the algorithm owner | Missing or conflicting context: stop and mark the claim UNKNOWN |
| Is a target built, wired, or runtime verified? | Route reachability questions through [`built-not-wired`](../built-not-wired/SKILL.md); classify manifest/workflow wiring separately from an actual build or run | Missing target, build event, or complete workflow population: HALT rather than infer reachability |
| Who reviews or approves this package? | Route the final artifact to an independent `review` / cold-review gate; this author has no approval authority | No verified reviewer or source owner: keep the artifact blocked and name no person |

<a id="s04"></a>
## S04 — Decisions and invariants

| Condition | Action and evidence | Stop when |
|---|---|---|
| Harness input offsets or thresholds change | Update the exact target matrix and check minimized-input compatibility in M04/M05 | The new input contract or retained corpus is unknown |
| A comment claims injectivity or key separation | Compare its claim with the actual assertions in [INV-004](../../../docs/ownership/crates/corelink-tenant-path-fuzz/REFERENCE.md#inv-004) | The desired cryptographic property has no sound test design |
| Parent derivation API changes | Coordinate with `corelink-tenant-path`; trace static dependents and cross-language vectors in B04 | Prefix compatibility or storage recovery is undecided |
| A workflow selects a fuzz target | Record trigger, target, limit, and actual run evidence as separate facts | Only YAML configuration is available but a run is being claimed |
| A request asks for service, provider, or production confirmation | Route to its verified operational owner when identified | No owner or authorized evidence path is known |

<a id="s05"></a>
## S05 — Workflow

1. Confirm the isolated checkout, integration baseline, source pin, and exact manifest path. 2. Read the manifest and both harnesses; classify every input slice and assertion. 3. Open only the parent API and consumer/workflow boundaries affected by the change. 4. Reconcile every dependency, target, workflow selection, and outside-Cargo parity boundary in B03–B06. 5. Select source-only procedures in M02; record separately any later authorized execution evidence. 6. Update only this package's

four artifacts and request a fresh independent review of their final bytes.

<a id="s06"></a>
## S06 — Stop conditions

Stop on source drift, an unresolved package identity, missing target inputs, a changed assertion whose predicate is unclear, or an unclassified stored-prefix compatibility effect. Stop if source configuration is presented as a successful run, or if a local fuzzer result is presented as service or production evidence. Do not infer that the extended target proves injectivity or that a UUID-v7 feature forces v7 inputs.

<a id="s07"></a>
## S07 — Evidence and handoff

Report the two baselines, changed target/API/REL/INV/PROC IDs, exact file paths and diff scope, checker results, and any real execution evidence with its environment and limits. State `not run` for fuzzing unless a source-linked run record exists. Name no person or team without verified attribution. The author does not approve these artifacts.

[Back to triggers](#s01)
