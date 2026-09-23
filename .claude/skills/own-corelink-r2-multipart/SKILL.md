---
name: own-corelink-r2-multipart
description: >-
  Use when changing the corelink-r2-multipart source-defined multipart trait,
  in-memory or failing fakes, canonical object-key construction, bounded part
  values, or per-tenant concurrency surface. Do not use to claim a real R2
  provider, worker wiring, storage execution, deployment, or operational sweep.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-r2-multipart"
  manifest: "crates/corelink-r2-multipart/Cargo.toml"
  source-commit: "6ed297f5b2b64cf97447985111a2ecbbaa9536bb"
  evidence-set: "r2-multipart-static-source-20260920"
---

# Ownership — corelink-r2-multipart

Candidate static ownership guide. It records source-visible trait, fake, key,
and semaphore contracts. It establishes neither a Cloudflare R2 implementation
nor provider execution, worker mounting, storage effects, deployment, or review
approval.

[Trigger](#s01) · [Boundary](#s02) · [Read](#s03) · [Keys](#s04) ·
[Lifecycle](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A multipart trait, fake, key composer, bound, or semaphore surface changes | Identify the matching source contract and invariant first | [R03](../../../docs/ownership/crates/corelink-r2-multipart/REFERENCE.md#r03), [R05](../../../docs/ownership/crates/corelink-r2-multipart/REFERENCE.md#r05) | The request requires an R2 call, worker route, or service behavior claim |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ownership is uncertain | Keep ownership to `src/{adapter,in_memory,object_key,concurrency,bounds,types,error,failover}.rs` and public re-exports | Manifest and [R02](../../../docs/ownership/crates/corelink-r2-multipart/REFERENCE.md#r02) | Treat a future `MultipartAdapter` provider shim, D1 schema, or consumer as owned here |

<a id="s03"></a>
## S03 — Read routing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A public symbol or static impact needs assessment | Read R03/R04, then the relevant B relation | [R03](../../../docs/ownership/crates/corelink-r2-multipart/REFERENCE.md#r03), [B02–B05](../../../docs/ownership/crates/corelink-r2-multipart/BLAST_RADIUS.md#b02) | Infer a complete reverse graph, selected Cargo features, or runtime path |

<a id="s04"></a>
## S04 — Canonical key decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing initiation inputs or object-key validation | Preserve typed `TenantPrefix`, validated region/digest/suffix inputs, and the composer route | [INV-003](../../../docs/ownership/crates/corelink-r2-multipart/REFERENCE.md#inv-003), [REL-003](../../../docs/ownership/crates/corelink-r2-multipart/BLAST_RADIUS.md#rel-003) | Claim a key was stored, accepted by R2, or protects another component |

<a id="s05"></a>
## S05 — Lifecycle and concurrency decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing trait/fake lifecycle, parts, or permits | Compare handle tenant binding, caller-provided completion-vector order, duplicate rejection, bounded part constructor, and per-tenant permit source | [INV-001](../../../docs/ownership/crates/corelink-r2-multipart/REFERENCE.md#inv-001), [INV-004](../../../docs/ownership/crates/corelink-r2-multipart/REFERENCE.md#inv-004), [REL-004](../../../docs/ownership/crates/corelink-r2-multipart/BLAST_RADIUS.md#rel-004) | Claim monotonic-order validation, actual R2 idempotency, committed objects, fairness, or throughput |

<a id="s06"></a>
## S06 — Stop conditions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Work needs AWS/Cloudflare SDK calls, D1 state, sweeper operation, credentials, or deployment | Isolate the requested integration and route it to its implementation/system owner | [R07](../../../docs/ownership/crates/corelink-r2-multipart/REFERENCE.md#r07), [B06](../../../docs/ownership/crates/corelink-r2-multipart/BLAST_RADIUS.md#b06) | Present fake/trait source as provider, persistence, or operational evidence |

<a id="s07"></a>
## S07 — Static handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A source-static assessment is complete | Report baseline, changed source contracts, observed relations, validation results, and explicit unknowns | [M06](../../../docs/ownership/crates/corelink-r2-multipart/MAINTENANCE.md#m06) | Present documentary checks as Cargo execution, test success, runtime proof, or cold review |

[Back to trigger](#s01)
