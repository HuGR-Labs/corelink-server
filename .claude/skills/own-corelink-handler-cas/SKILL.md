---
name: own-corelink-handler-cas
description: >-
  Route source-backed ownership changes to the corelink-handler-cas trait,
  envelope, local audit/SLI seam, digest-tag, and in-memory-fake package.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-handler-cas
  manifest: crates/corelink-handler-cas/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-handler-cas-structural-normalization-20260921
---

# Ownership — corelink-handler-cas

This is a source/static ownership guide. It does not certify a concrete
storage provider, HTTP/REAPI mount, wasm target, audit persistence, metric
delivery, deployment, or independent review.

[Trigger](#s01) · [Boundary](#s02) · [Read](#s03) · [Traits](#s04) ·
[Fake](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A public request, response, error, trait, audit, observer, or digest tag changes | Open the manifest and the matching local module before editing | `Cargo.toml`; [R02–R06](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#r02) | The requested change is a route, provider, or deployment decision |
| A caller needs a behavior guarantee | Separate the trait's documented requirement from the fake implementation | `src/handler.rs`; [R07](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#r07) | The only evidence is a comment, fake, or unrun test |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Assigning ownership | Keep ownership to this package's local modules, exported traits/envelopes, and in-memory collaborators | `src/lib.rs`; `src/{audit,digest_algo,error,handler,observer,request}.rs` | Attribute `corelink-slo`, a storage backend, or container composition to this crate |
| A concrete implementation changes | Coordinate with its owning package and retain this package's trait compatibility | [B02–B05](../../../docs/ownership/crates/corelink-handler-cas/BLAST_RADIUS.md#b02) | A manifest edge is treated as evidence of a running path |

<a id="s03"></a>
## S03 — Read routing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Reviewing public shape | Read `lib.rs`, `request.rs`, `error.rs`, and [R02–R03](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#r02) | Re-exports, `#[non_exhaustive]` types, constructors, and error variants | Infer an HTTP status or mounted route |
| Reviewing audit or telemetry | Read `audit.rs`, `observer.rs`, trait documentation, and fake control flow | [R04](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#r04), [R07](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#r07) | Claim durable audit acceptance or delivered SLO data |
| Reviewing digest selection | Read `digest_algo.rs` and `handler.rs` helpers | [R05](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#r05) | Substitute `fake_hash` for BLAKE3, storage verification, or REAPI conformance |

<a id="s04"></a>
## S04 — Trait and envelope decisions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Read/write/delete/list contract changes | Preserve explicit tenant, caller, accounting, digest, page, and response fields; examine every trait implementor in scope | `request.rs`; `handler.rs`; [INV-001](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#inv-001) | Compatibility of all downstream callers is not source-demonstrated |
| Changing `exists` or `exists_batch` | Preserve `NotFound` absorption, ordered full-batch flags, and the default `None` capability result | `handler.rs`; [INV-002](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#inv-002) | The default read fallback is called a cheap storage HEAD |
| Changing audit ordering | Distinguish pre-mutation attempt/denial rows from post-mutation committed rows and audit-failure paths | `audit.rs`; `handler.rs`; [INV-003](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#inv-003) | A trait comment is treated as proof every implementation or failure path obeys it |

<a id="s05"></a>
## S05 — Fake and digest decisions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing the in-memory fake | Preserve its source-defined in-memory-fake and temporary native wire-up boundary; inspect `seed`, injection, read, write, delete, and list separately | `handler.rs`; `lib.rs`; [R07](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#r07) | Present a fake behavior as durable-storage behavior |
| Changing `DigestAlgo` or helpers | Keep the explicit `Blake3`/`Sha256` selector; verify SHA-256 with `sha2`/`hex`; label `fake_hash` synthetic | `digest_algo.rs`; `handler.rs`; [INV-004](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#inv-004) | Infer digest algorithm from text length or call the fake BLAKE3 |

<a id="s06"></a>
## S06 — Stop conditions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Work needs R2/S3/KV, HTTP, Axum, REAPI wire behavior, or a wasm implementation | Hand off to the concrete provider/composition owner | [R08](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#r08), [B06](../../../docs/ownership/crates/corelink-handler-cas/BLAST_RADIUS.md#b06) | Add or imply a provider implementation here |
| Work needs audit durability, SLO aggregation, traffic, credentials, or deploy state | Request appropriate operational evidence | `AuditSink`/`SliObserver` are local traits; [R08](../../../docs/ownership/crates/corelink-handler-cas/REFERENCE.md#r08) | Treat in-memory capture as production evidence |

<a id="s07"></a>
## S07 — Static handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A static ownership change is ready | Report baseline, changed symbols, direct static relations, structural-check results, and unknowns | [M06](../../../docs/ownership/crates/corelink-handler-cas/MAINTENANCE.md#m06) | Present documentary validation as a build, runtime result, or cold review |

[Back to trigger](#s01)
