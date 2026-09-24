---
name: own-corelink-analytics
description: >-
  Assume ownership of corelink-analytics when changing RED metric taxonomy,
  typed labels, cardinality validation, observer traits, analytics audit
  envelopes, configuration, or canonical migration 0015. Do not use for
  production Analytics Engine, Prom remote-write, deployment, or runtime proof.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-analytics"
  manifest: "crates/corelink-analytics/Cargo.toml"
  source-commit: "59c76cf260bcdeb5246772f70821ac8b7e8a9780"
  evidence-set: "analytics-static-source-20260920"
---

# Ownership — corelink-analytics

Candidate ownership guide grounded only in the checked source and static Cargo graph. It does not establish runtime wiring, deployment, review, or service operation.

[Trigger](#s01) · [Boundary](#s02) · [Read](#s03) · [Taxonomy](#s04) · [Validator](#s05) · [Migration](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A metric, label, observer, validator, audit record, config, or migration 0015 changes | Open the matching reference record before editing | `src/{canonical,labels,observer,validator,audit,config}.rs`; `src/lib.rs` | Requested work is production wiring or deploy |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ownership is uncertain | Keep scope to the crate’s exported pure-logic surface and its embedded SQL | `src/lib.rs` exports; manifest names only `corelink-eviction` as direct internal dependency | Treat a workspace consumer or SQL commentary as runtime/export proof |

<a id="s03"></a>
## S03 — Read routing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Need a contract or known consumer | Read [reference](../../../docs/ownership/crates/corelink-analytics/REFERENCE.md#r04) then [relations](../../../docs/ownership/crates/corelink-analytics/BLAST_RADIUS.md#b04) | Export list in `src/lib.rs`; Cargo manifests | A consumer is absent from the static search result |

<a id="s04"></a>
## S04 — Taxonomy and labels

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Adding or changing metric names, kinds, tiers, regions, or labels | Preserve typed canonical vocabulary and re-evaluate forbidden high-cardinality names | `src/canonical.rs`; `src/labels.rs`; [R04](../../../docs/ownership/crates/corelink-analytics/REFERENCE.md#r04) | Label cardinality or downstream compatibility is not bounded by source evidence |

<a id="s05"></a>
## S05 — Validator and audit envelope

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing admission or failure behavior | Trace audit emission and state mutation through validator and observer | `src/validator.rs`; `src/observer.rs`; `src/audit.rs` | Claim a handler returns a particular HTTP response without its handler source |

<a id="s06"></a>
## S06 — Configuration and migration

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing budgets, histogram boundaries, or schema version | Reconcile config constants, embedded migration, and migration-shape test | `src/config.rs`; `src/lib.rs`; `migrations/d1/0015_analytics_cardinality_budgets.sql`; `tests/migration_canonical_0015.rs` | Assume the D1 migration has been applied or hydrated at runtime |

<a id="s07"></a>
## S07 — Handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Completing a static ownership change | Report baseline, changed records, static consumer set, and unknowns | [maintenance](../../../docs/ownership/crates/corelink-analytics/MAINTENANCE.md#m06) and source paths inspected | Present documentation checks as semantic, runtime, or review approval |

[Back to trigger](#s01)
