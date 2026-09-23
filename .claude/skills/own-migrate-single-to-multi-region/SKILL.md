---
name: own-migrate-single-to-multi-region
description: >-
  Route static source and contract work for the migrate-single-to-multi-region
  binary, preserving the boundary between its local stubs and real migration operations.
metadata:
  schema: "corelink-ownership/1.1"
  package: "migrate-single-to-multi-region"
  manifest: "apps/migrate-single-to-multi-region/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "w015-migrate-single-to-multi-region-static-20260921"
---

# Ownership — migrate-single-to-multi-region

[Activation](#s01) · [Authority](#s02) · [Reading](#s03) ·
[Decisions](#s04) · [Workflow](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Activation

| Use this skill when | Do not use it when |
|---|---|
| Reviewing the binary manifest, argument parser, mode dispatch, local reports, or its corelink-region interface. | A request requires tenant migration, rollback, Terraform, D1, R2, provider, or production activity. |

<a id="s02"></a>
## S02 — Territory and authority

**Implementation owned:** the package manifest and local source in [main.rs](../../../apps/migrate-single-to-multi-region/src/main.rs): Args parsing, dispatch, source-local report construction, output, and printed rollback text.

**Imported contract owner:** [corelink-region](../../../docs/ownership/crates/corelink-region/REFERENCE.md#r01) defines the imported Region, migration-report, result, and audit interfaces.

**Composition and operation:** no source-backed provider composition root or verified operator route is established here. The RB-region document is a procedure reference, not authorization.

Runtime, data, and operational owners remain unknown.

**Review:** an independent cold review is pending; this skill does not grant access or approve its own changes.

<a id="s03"></a>
## S03 — Reading route

| Question | Read |
|---|---|
| What does this package declare and implement? | [Reference](../../../docs/ownership/crates/migrate-single-to-multi-region/REFERENCE.md#r01) |
| Which contracts and outside-Cargo edges matter? | [Blast radius](../../../docs/ownership/crates/migrate-single-to-multi-region/BLAST_RADIUS.md#b01) |
| How may a source-only change be reviewed? | [Maintenance](../../../docs/ownership/crates/migrate-single-to-multi-region/MAINTENANCE.md#m01), then the independent [cold-review route](../../../docs/ownership/WAVE_015_PLAN.md) |
| Which architecture context must be loaded first? | [`okf-context`](../okf-context/SKILL.md), then [`built-not-wired`](../built-not-wired/SKILL.md) for shipped-target claims |
| Which adjacent package owns imported contracts? | [`own-corelink-region`](../own-corelink-region/SKILL.md) and its [reference](../../../docs/ownership/crates/corelink-region/REFERENCE.md#r01) |
| Where is the canonical concept routed? | [Wave 015 plan](../../../docs/ownership/WAVE_015_PLAN.md) and the canonical OKF route named there |

Before a new source-area review, load the matching concepts with `python3 scripts/okf_context.py --file apps/migrate-single-to-multi-region/src/main.rs` (and `--full` for returned concepts). Use `built-not-wired` whenever a conclusion concerns a selected image, workflow, or shipped binary. The OKF route is reference-only; do not duplicate or redefine its policy.

<a id="s04"></a>
## S04 — Decisions and invariants

| Condition | Action and evidence | Stop when |
|---|---|---|
| Mode, argument, report, or imported type changes | Trace the exact branch in main.rs and its API/INV/REL entry; label evidence SOURCE. | A conclusion needs a build, invocation, provider, tenant, or runtime observation. |
| A request cites --dry-run or --execute | Treat the flag as an advertised/source branch only. Dry-run and execute use hard-coded local fixtures and an in-memory sink; do not infer D1/R2/hash/audit behavior. Never invoke a mode under this ownership task. | Any action would touch customer data, D1, R2, Terraform, or production. |
| A request cites --rollback, or asks to “test” rollback | Treat rollback as a printed manual-instruction branch, not a rollback operation. Route any real restore request to the verified operational owner and authorization path; none is established here. | A request needs Terraform state, D1 PITR, R2 versions, routing verification, or recovery authority. |

Preserve these five axioms: the package name comes from Cargo package.name; a declared test is not an executed test; a dependency or import is not runtime reachability; fake or in-memory behavior is not provider behavior; advertised migration modes are not permission to execute them.

<a id="s05"></a>
## S05 — Workflow

1. Confirm package identity, source pin, integration baseline, and the four assigned paths. 2. Read the changed parser, branch, or imported contract in the pinned source and inspect the matching REL. 3. Search Cargo reverse edges and non-Cargo invocation, build, CI, runbook, and data references; record search limits. 4. Change only the assigned ownership documents; label all command and execution states separately. 5. Run only the supplied static-document checker and

whitespace/scope checks requested for the ownership task; request independent cold review of final bytes.

<a id="s06"></a>
## S06 — Stop conditions

Stop before any binary mode, Cargo/Rust/test/build/fuzz command, network or provider access, GitHub action, deployment, or tenant/storage/Terraform/database change. Stop if a claim depends on live regions, audit delivery, data movement, hash verification, rollback completion, or an unverified owner. Keep the claim unknown and route only through an independently verified owner or authorization path; none is supplied here for live migration.

<a id="s07"></a>
## S07 — Handoff

Report the source pin and baseline, exact changed paths, API/INV/REL/PROC IDs, static commands and results, skipped operational checks, unresolved owners, and review state. Distinguish source stubs from provider behavior. Do not claim that the binary compiled, ran, migrated tenants, emitted external audit, or restored state unless separate evidence establishes that fact.

[Back to start](#s01)
