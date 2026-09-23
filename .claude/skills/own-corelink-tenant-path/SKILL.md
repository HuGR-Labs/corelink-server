---
name: own-corelink-tenant-path
description: Maintain source-only ownership records for the corelink-tenant-path package and its typed tenant-prefix derivation boundary.
metadata:
  evidence-set: w007-tenant-path-static-source-20260920
  source-commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
  package: corelink-tenant-path
  manifest: crates/tenant-path/Cargo.toml
  profile: S
  evidence_mode: SOURCE
---

# Own `corelink-tenant-path`

Use this guide only for the package named `corelink-tenant-path` at `crates/tenant-path/Cargo.toml`. It records static source contracts, not tenant, storage, cache, key-management, deployment, or runtime execution.

[Scope](#s01) · [Identity](#s02) · [Derivation](#s03) · [Types](#s04) · [Cache](#s05) · [Relations](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Scope decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Request concerns this package's static contract | Read manifest and named source only | `Cargo.toml`, `src/{lib,prefix,cache,error}.rs` | Stop if it asks for runtime or storage proof |

<a id="s02"></a>
## S02 — Identity decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Package naming is relevant | Use `corelink-tenant-path`, never the directory basename as identity | `Cargo.toml` package name | Stop on a different manifest |

<a id="s03"></a>
## S03 — Derivation decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `derive_prefix` or its output changes | Trace typed inputs, HMAC call, encoding, and fixed-width copy | `src/prefix.rs` | Stop before asserting collision, authorization, or live-key behavior |

<a id="s04"></a>
## S04 — Type-boundary decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Public types or re-exports change | Check visibility, constructors, and formatting methods | `src/lib.rs`, `src/prefix.rs` | Stop if a caller's serialization or secret handling must be proved |

<a id="s05"></a>
## S05 — Cache decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Cache API or key changes | Trace `(TdkVersion, Uuid)` lookup and fallback to direct derivation | `src/cache.rs` | Stop before making isolate, rotation, memory, or concurrency claims |

<a id="s06"></a>
## S06 — Relationship decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A consumer-facing change is proposed | Record direct manifest edge or import separately from use/execution | consumer manifest/source | Stop if reverse-dependency completeness or runtime reachability is required |

<a id="s07"></a>
## S07 — Handoff decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static documentation is ready | Supply baseline, changed paths, R/B/M IDs, checks, and unknowns | four ownership artifacts | Stop on a failed checker, scope breach, or need for execution evidence |

Success means accurate package identity and bounded SOURCE claims. Completeness means S01–S07 map decisions to evidence and stops. Quality means source comments are not elevated to runtime fact. DoD requires the companion [reference](../../../docs/ownership/crates/corelink-tenant-path/REFERENCE.md#r01), [blast radius](../../../docs/ownership/crates/corelink-tenant-path/BLAST_RADIUS.md#b01), and [maintenance](../../../docs/ownership/crates/corelink-tenant-path/MAINTENANCE.md#m01) records plus structural validation; no execution is claimed.
