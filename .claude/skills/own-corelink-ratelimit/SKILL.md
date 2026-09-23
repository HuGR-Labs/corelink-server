---
name: own-corelink-ratelimit
description: Use when changing static source contracts for the corelink-ratelimit token-bucket and fake limiter crate.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-ratelimit"
  manifest: "crates/corelink-ratelimit/Cargo.toml"
  source-commit: "6ed297f5b2b64cf97447985111a2ecbbaa9536bb"
  evidence-set: "ratelimit-static-source-20260920"
  profile: "S"
---

# Ownership — corelink-ratelimit

Use this static ownership guide for local token-bucket source, its trait, and
in-memory collaborators. SOURCE is not executed or runtime evidence; it does
not establish any DO, D1, provider, HTTP, deployment, or rate-limit operation.

[Trigger](#s01) · [Boundary](#s02) · [Contracts](#s03) · [Fake](#s04) · [Policy route](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Beginning a change | Compare revision, manifest, crate root, and affected modules | `Cargo.toml`, `src/lib.rs`, local `src/*.rs` | Baseline differs or proof requires execution |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Request names storage, actors, or middleware | Own source declarations, pure bucket logic, trait surface, and fake only | [R02](../../../docs/ownership/crates/corelink-ratelimit/REFERENCE.md#r02) | Treat comments, embedded SQL, or trait names as operating DO/D1 |

<a id="s03"></a>
## S03 — Contracts

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Public type, decision, key, error, or config changes | Follow root re-export to defining module; preserve source compatibility predicates | [R04](../../../docs/ownership/crates/corelink-ratelimit/REFERENCE.md#r04) | Infer wire behavior, caller behavior, or selected features |

<a id="s04"></a>
## S04 — Fake

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Editing in-memory limiter or capture sink | Inspect mutex/map and injected traits as local fake contracts | [R03](../../../docs/ownership/crates/corelink-ratelimit/REFERENCE.md#r03) | Call it durable, distributed, provider-backed, or runtime-representative |

<a id="s05"></a>
## S05 — Policy route

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Change concerns tier, retry, audit, metrics, or migration policy | Use verified OKF routing; record only the local source relation | `docs/ownership/WAVE_008_PLAN.md`; [R05](../../../docs/ownership/crates/corelink-ratelimit/REFERENCE.md#r05) | Copy, redefine, or revalidate canonical policy here |

<a id="s06"></a>
## S06 — Stops

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Proof needs Cargo, tests, network, provider state, deployment, or production | State missing evidence and request a separately scoped procedure | [B06](../../../docs/ownership/crates/corelink-ratelimit/BLAST_RADIUS.md#b06) | Upgrade SOURCE into executed/runtime/DO/D1 claims |

<a id="s07"></a>
## S07 — Handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static work is complete | Report baseline, paths, relations, documentary checks, and unknowns | [M06](../../../docs/ownership/crates/corelink-ratelimit/MAINTENANCE.md#m06) | Present documentation validation as build, test, provider, or production proof |

[Back to trigger](#s01)
