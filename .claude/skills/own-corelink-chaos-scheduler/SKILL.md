---
name: own-corelink-chaos-scheduler
description: >-
  Route source-only ownership changes to corelink-chaos-scheduler's catalog,
  deterministic runner, safe-mode predicates, telemetry trait, and local queue;
  exclude scheduled or runtime chaos operation.
metadata:
  schema: "corelink-ownership/1.1"
  profile: "S"
  package: "corelink-chaos-scheduler"
  manifest: "crates/corelink-chaos-scheduler/Cargo.toml"
  source-commit: "3feae2baed63061354533ffdfe2d94acfd24aa9a"
  evidence-set: "chaos-scheduler-static-source-20260920"
---

# Ownership — corelink-chaos-scheduler

Static-source routing guide. SOURCE describes declarations and local branch
predicates at the pinned baseline; it does not prove a chaos run, cron fire,
telemetry delivery, rollback, archive, deployment, or production behavior.
The [verified OKF profile](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md)
is a route only and is neither copied nor revalidated here.

[Trigger](#s01) · [Boundary](#s02) · [Catalog](#s03) · [Runner](#s04) · [Queue](#s05) · [Unknowns](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A package contract changes | Route it to the named static records | `Cargo.toml`; `src/{lib,catalog,runner,types}.rs` | The request needs a scheduled or observed chaos run |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Scope is uncertain | Retain catalog values, pure runner ordering, trait signatures, and in-memory queue semantics | [R01–R07](../../../docs/ownership/crates/corelink-chaos-scheduler/REFERENCE.md#r01) | Infer a cron, provider, metric, audit sink, rollback, or environment |

Five static axioms: (1) manifest and source prove declarations only; (2)
public signatures prove source-visible contracts only; (3) a trait proves
neither an implementation nor invocation; (4) local branches and the in-memory
queue prove no scheduled run or external effect; (5) the verified OKF route is
routing context only.

<a id="s03"></a>
## S03 — Catalog and types

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Catalog, identifier, taxonomy, or bound changes | Reconcile the exact source rows, labels, and `is_ga_mandatory` predicate | [R03](../../../docs/ownership/crates/corelink-chaos-scheduler/REFERENCE.md#r03); [B01](../../../docs/ownership/crates/corelink-chaos-scheduler/BLAST_RADIUS.md#b01) | Claim a named dependency was targeted or affected |

<a id="s04"></a>
## S04 — Safe-mode and runner

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Snapshot, seed, outcome, or event ordering changes | Preserve the ordered static predicates and an input that falsifies each | [R04](../../../docs/ownership/crates/corelink-chaos-scheduler/REFERENCE.md#r04); [B02–B04](../../../docs/ownership/crates/corelink-chaos-scheduler/BLAST_RADIUS.md#b02) | Present a branch as a live safety control or delivered event |

<a id="s05"></a>
## S05 — Telemetry and queue

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `Telemetry`, `ChaosScheduler`, or rotation changes | Map the abstract call/queue relation and source-local FIFO/modulo rule | [R05](../../../docs/ownership/crates/corelink-chaos-scheduler/REFERENCE.md#r05); [B05](../../../docs/ownership/crates/corelink-chaos-scheduler/BLAST_RADIUS.md#b05) | Assert an adapter, persistence, lock behavior, or schedule exists |

<a id="s06"></a>
## S06 — Unknowns and escalation

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A conclusion needs an uninspected boundary | Record it as unknown and route it to the accountable integration or operations owner | [R08](../../../docs/ownership/crates/corelink-chaos-scheduler/REFERENCE.md#r08); [B06](../../../docs/ownership/crates/corelink-chaos-scheduler/BLAST_RADIUS.md#b06) | Fill an evidence gap with comments, fakes, or intent |

<a id="s07"></a>
## S07 — Static handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static assessment is complete | Report baseline, source paths, affected R/B/M IDs, structural checks, and unknowns | [M06](../../../docs/ownership/crates/corelink-chaos-scheduler/MAINTENANCE.md#m06) | Call author checks a Cargo, test, runtime, or independent-review result |

Success means each source claim has a falsifier. Completeness means S01–S07,
R01–R08, B01–B06, and M01–M06 agree. Quality means static relationships remain
atomic and execution claims remain unknown. DoD is these four ownership
artifacts plus structural outcomes; it implies no scheduled or runtime result.

```sh
python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-chaos-scheduler/SKILL.md
python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-chaos-scheduler/REFERENCE.md
python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-chaos-scheduler/BLAST_RADIUS.md
python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-chaos-scheduler/MAINTENANCE.md
git diff --check 3feae2baed63061354533ffdfe2d94acfd24aa9a..HEAD
```

The checked-in checker results and whitespace check are structural-only,
never semantic approval or execution evidence. [Reference](../../../docs/ownership/crates/corelink-chaos-scheduler/REFERENCE.md#r01) · [Impact map](../../../docs/ownership/crates/corelink-chaos-scheduler/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-chaos-scheduler/MAINTENANCE.md#m01)
